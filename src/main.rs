use std::{time::Instant, sync::{Arc, Mutex}};

use actix::{
    self, Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message, StreamHandler
};
use actix_cors::Cors;
use actix_web::{
    middleware::Logger, web::{self, Query}, App, Error, HttpRequest, HttpResponse, HttpServer, Responder
};
use actix_web_actors::ws::{self, WebsocketContext};
use serde::{Deserialize, Serialize};
use serde_json;

mod database;
mod types;

use database::Database;

struct Server {
    database: Arc<Mutex<Database>>,
}

impl Server {
    #[allow(unused)]
    pub fn new(db: &Arc<Mutex<Database>>) -> Server {
        Server {
            database: Arc::clone(&db)
        }
    }
}

#[derive(Serialize, Deserialize)]
enum JsonMessage {
    ActivationMessage((String, String)),
    SendMail(ForwardMessage),
    Empty,
}

impl Actor for Server {
    type Context = Context<Server>;
}

#[derive(Message)]
#[rtype(Result = "()")]
struct ActiveMessage {
    identifier: String,
    password: String,
    addr: Addr<Client>,
}

#[derive(Message, Serialize, Deserialize)]
#[rtype(Result = "()")]
struct Disconnect {
    id: String,
}

#[derive(Message, Serialize, Deserialize)]
#[rtype(Result = "()")]
struct ForwardMessage {
    pub next: String,
    pub mail: String,
}

impl Handler<ActiveMessage> for Server {
    type Result = ();
    fn handle(&mut self, msg: ActiveMessage, _ctx: &mut Self::Context) -> Self::Result {
        println!(
            "Activation requested by: {} with {}",
            &msg.identifier, &msg.password
        );
        if self.database.lock().unwrap().is_alive(&msg.identifier).is_ok() {
            return;
        }
        self.database
            .lock().unwrap()
            .activate_self(msg.identifier, msg.password, msg.addr)
            .unwrap();
    }
}

impl Handler<Disconnect> for Server {
    type Result = ();
    fn handle(&mut self, msg: Disconnect, _ctx: &mut Self::Context) -> Self::Result {
        self.database.lock().unwrap().deactivate(msg.id);
    }
}

impl Handler<ForwardMessage> for Server {
    type Result = ();
    fn handle(&mut self, msg: ForwardMessage, _ctx: &mut Self::Context) -> Self::Result {
        match self.database.lock().unwrap().is_alive(&msg.next) {
            Ok(status) => match status {
                true => match self.database.lock().unwrap().get_addr(msg.next.clone()) {
                    Some(addr) => {
                        addr.do_send(msg);
                        // figure out a way to send a "delivered" reply to the sender
                    }
                    None => {}
                },
                false => {
                    self.database.lock().unwrap().save_mail_for(msg.next, msg.mail).unwrap();
                }
            },
            Err(error) => println!("{:?}", error),
        }
    }
}

#[allow(unused)]
pub struct Client {
    pub id: String,
    addr: Addr<Server>,
    pub hb: Instant,
}

impl Client {
    fn new(addr: Addr<Server>) -> Client {
        Client {
            id: String::new(),
            addr: addr.clone(),
            hb: Instant::now(),
        }
    }
}

impl Actor for Client {
    type Context = WebsocketContext<Self>;
}

impl Handler<ForwardMessage> for Client {
    type Result = ();
    fn handle(&mut self, msg: ForwardMessage, ctx: &mut Self::Context) -> Self::Result {
        //ctx.binary(msg.mail.payload)
        ctx.text(msg.mail)
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Client {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Binary(bin)) => {
                ctx.binary(bin);
            }
            Ok(ws::Message::Text(message)) => {
                println!("message from server: {}", message);
                // parse JSON
                let json_message = serde_json::from_str::<JsonMessage>(&message);
                match json_message {
                    Ok(JsonMessage::SendMail(mail)) => self.addr.do_send(mail),
                    Ok(JsonMessage::ActivationMessage(activation)) => {
                        self.addr.do_send(ActiveMessage {
                            identifier: activation.0,
                            password: activation.1,
                            addr: ctx.address().clone(),
                        })
                    }
                    Ok(JsonMessage::Empty) => self.hb = Instant::now(),
                    Err(err) => println!("error while parsing message: {}", err),
                }
            }
            Ok(ws::Message::Close(reason)) => {
                self.addr.do_send(Disconnect {
                    id: self.id.clone(),
                });
                ctx.close(reason);
                ctx.stop();
            }
            Err(err) => {
                println!("Error: {}", err);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

async fn index(
    req: HttpRequest,
    stream: web::Payload,
    server_addr: web::Data<Addr<Server>>,
) -> Result<HttpResponse, Error> {
    println!("Connection");
    let res = ws::start(Client::new(server_addr.get_ref().clone()), &req, stream)?;
    Ok(res)
}

#[derive(Serialize, Deserialize)]
struct User {
    identifier: String,
    password: String
}

#[actix_web::get("/messages")]
async fn get_mailbox(
    _req: HttpRequest,
    query: Query<User>,
    database: web::Data<Arc<Mutex<Database>>>,
) -> impl Responder {
    match database.lock().unwrap().get_mails_for(&query.identifier) {
        Ok(mail) => serde_json::to_string(&mail).unwrap(),
        Err(_err) => "Failed to get mail".to_owned()
    }
}

#[actix::main]
async fn main() -> std::io::Result<()> {
    let connection = Mutex::new(Database::new("data.db".to_owned()));
    let connection = Arc::new(connection);
    let server = Server::new(&connection);
    let addr = server.start();
    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("localhost:8080")
            .allow_any_method()
            .allow_any_header();
        App::new()
            .wrap(Logger::default())
            .wrap(cors)
            .app_data(web::Data::from(Arc::clone(&connection)))
            .app_data(web::Data::new(addr.clone()))
            .service(web::resource("/").to(index))
            .service(get_mailbox)
    })
    .bind("localhost:4000")?
    .run()
    .await?;
    Ok(())
}
