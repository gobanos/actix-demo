use actix::fut::wrap_future;
use actix::prelude::*;
use actix_web::server;
use actix_web::App;
use actix_web::AsyncResponder;
use actix_web::FutureResponse;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use futures::future;
use futures::Future;

struct SyncActor;
struct AsyncActor(Addr<SyncActor>);

fn sync_fn(a: i32) -> i32 {
    a * 2
}

struct Msg(i32);
struct Msg2(i32);

impl Message for Msg {
    type Result = i32;
}

impl Message for Msg2 {
    type Result = Result<i32, MailboxError>;
}

impl Actor for SyncActor {
    type Context = SyncContext<Self>;
}

impl Handler<Msg> for SyncActor {
    type Result = i32;

    fn handle(&mut self, msg: Msg, _ctx: &mut Self::Context) -> Self::Result {
        println!("SyncActor: {}", msg.0);

        sync_fn(msg.0)
    }
}

impl Actor for AsyncActor {
    type Context = Context<Self>;
}

impl Handler<Msg2> for AsyncActor {
    type Result = ResponseActFuture<Self, i32, MailboxError>;

    fn handle(&mut self, msg: Msg2, _ctx: &mut Self::Context) -> Self::Result {
        println!("AsyncActor: {}", msg.0);

        Box::new(wrap_future(self.0.send(Msg(msg.0 * 2))))
    }
}

fn main() {
    let sys = System::new("actix-demo");

    server::new(move || {
        let sync_addr = SyncArbiter::start(3, || SyncActor);
        let async_addr = AsyncActor(sync_addr.clone()).start();

        App::with_state((sync_addr, async_addr)).resource("/", |r| r.f(index))
    })
    .bind("127.0.0.1:8080")
    .unwrap()
    .start();

    println!("Hello, world!");

    let _ = sys.run();
}

fn index(req: &HttpRequest<(Addr<SyncActor>, Addr<AsyncActor>)>) -> FutureResponse<HttpResponse> {
    let futures: Vec<Box<Future<Item = i32, Error = _>>> = vec![
        Box::new(req.state().0.send(Msg(21))),
        Box::new(req.state().1.send(Msg2(21)).map(|x| x.unwrap_or(0))),
    ];

    future::join_all(futures)
        .from_err()
        .and_then(|_res| Ok(HttpResponse::Ok().body("It works !")))
        .responder()
}
