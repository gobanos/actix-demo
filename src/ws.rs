//! Simple echo websocket server.
//! Open `http://localhost:8080/ws/index.html` in browser
//! or [python console client](https://github.com/actix/examples/blob/master/websocket/websocket-client.py)
//! could be used for testing.

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{ws, Error, HttpRequest, HttpResponse};

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

lazy_static! {
    pub static ref WS_ADDRS: Arc<RwLock<HashMap<u64, Addr<MyWebSocket>>>> = Default::default();
}

/// do websocket handshake and start `MyWebSocket` actor
pub fn ws_index(r: &HttpRequest) -> Result<HttpResponse, Error> {
    println!("ws_index");
    ws::start(r, MyWebSocket::new())
}

pub struct WsMsg(pub i32);

impl Message for WsMsg {
    type Result = ();
}

/// websocket connection is long running connection, it easier
/// to handle with an actor
pub struct MyWebSocket {
    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    hb: Instant,
    id: u64,
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        let mut addrs = WS_ADDRS.write().unwrap();
        addrs.insert(self.id, ctx.address());

        ctx.text(format!("ID: {}", self.id));

        self.hb(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        let mut addrs = WS_ADDRS.write().unwrap();
        addrs.remove(&self.id);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<ws::Message, ws::ProtocolError> for MyWebSocket {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        // process websocket messages
        println!("WS: {:?}", msg);
        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => ctx.text(text),
            ws::Message::Binary(bin) => ctx.binary(bin),
            ws::Message::Close(_) => {
                ctx.stop();
            }
        }
    }
}

impl Handler<WsMsg> for MyWebSocket {
    type Result = ();

    fn handle(&mut self, msg: WsMsg, ctx: &mut Self::Context) -> Self::Result {
        ctx.text(format!("PROGRESS: {}", msg.0));
    }
}

impl MyWebSocket {
    fn new() -> Self {
        Self {
            hb: Instant::now(),
            id: rand::random(),
        }
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping("");
        });
    }
}
