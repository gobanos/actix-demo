use actix::fut::wrap_future;
use actix::prelude::*;
use actix_broker::BrokerIssue;
use actix_demo::ws_broker::{self, WsMsg};
use actix_web::{http, server, App, AsyncResponder, FutureResponse, HttpRequest, HttpResponse, fs};
use futures::{future, Future};
use rand::Rng;
use rayon::prelude::*;
use std::time::Duration;

#[derive(Clone)]
struct BrokerActor;

#[derive(Clone)]
struct SyncActor(Addr<BrokerActor>);

#[derive(Clone)]
struct AsyncActor(Addr<SyncActor>);

fn sync_fn(a: i32) -> i32 {
    let mut rng = rand::thread_rng();
    let sec = rng.gen_range(1, 5);
    println!("sync_fn: {}, sleep for {} sec", a, sec);
    std::thread::sleep(Duration::new(sec, 0));
    println!("sync_fn: {}, done", a);
    a * 2
}

struct Msg(i32, Option<u64>);

struct Msg2(i32, Option<u64>);

impl Message for Msg {
    type Result = ();
}

impl Message for Msg2 {
    type Result = Result<(), MailboxError>;
}

impl Actor for SyncActor {
    type Context = SyncContext<Self>;
}

impl Handler<Msg> for SyncActor {
    type Result = ();

    fn handle(&mut self, msg: Msg, _ctx: &mut Self::Context) -> Self::Result {
        let Msg(val, id) = msg;
        let addr = self.0.clone();

        println!("SyncActor: {}", val);

        std::thread::spawn(move || {
            (0..val)
                .into_par_iter()
                .map(move |i| {
                    let value = sync_fn(i);

                    if let Some(id) = id {
                        addr.do_send(WsMsg(value, id));
                    }
                })
                .count()
        });
    }
}

impl Actor for AsyncActor {
    type Context = Context<Self>;
}

impl Handler<Msg2> for AsyncActor {
    type Result = ResponseActFuture<Self, (), MailboxError>;

    fn handle(&mut self, msg: Msg2, _ctx: &mut Self::Context) -> Self::Result {
        println!("AsyncActor: {}", msg.0);

        Box::new(wrap_future(self.0.send(Msg(msg.0, msg.1))))
    }
}

impl Actor for BrokerActor {
    type Context = Context<Self>;
}

impl Handler<WsMsg> for BrokerActor {
    type Result = ();

    fn handle(&mut self, msg: WsMsg, _ctx: &mut Self::Context) -> Self::Result {
        self.issue_async(msg)
    }
}

fn main() {
    let sys = System::new("actix-demo");
    rayon::ThreadPoolBuilder::new()
        .num_threads(50)
        .build_global()
        .unwrap();

    server::new(move || {
        let broker_addr = BrokerActor.start();
        let sync_addr = SyncArbiter::start(3, move || SyncActor(broker_addr.clone()));
        let async_addr = AsyncActor(sync_addr.clone()).start();

        vec![
            App::new()
                .prefix("/ws")
                .resource("/", |r| r.method(http::Method::GET).f(ws_broker::ws_index))
                .boxed(),
            App::with_state((sync_addr, async_addr))
                .resource("/", |r| r.f(index))
                .handler("/test/", fs::StaticFiles::new("static/").unwrap())
                .boxed(),
        ]
    })
    .bind("127.0.0.1:8080")
    .unwrap()
    .start();

    println!("Hello, world!");

    let _ = sys.run();
}

fn index(req: &HttpRequest<(Addr<SyncActor>, Addr<AsyncActor>)>) -> FutureResponse<HttpResponse> {
    let id = req.query().get("id").and_then(|id| id.parse().ok());

    let futures: Vec<Box<Future<Item = (), Error = _>>> = vec![
        Box::new(req.state().0.send(Msg(10, id))),
        Box::new(req.state().1.send(Msg2(10, id)).map(|x| x.unwrap_or(()))),
    ];

    future::join_all(futures)
        .from_err()
        .and_then(|_res| Ok(HttpResponse::Ok().body("It works !")))
        .responder()
}
