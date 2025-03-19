#![allow(non_snake_case)]
#![allow(unused)]

use actix_cors::Cors;
use actix_web::cookie::time::macros::offset;
use sqlx::pool;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::fs::File;
use std::sync::Arc;
use tokio::sync::Mutex;
use sqlx::{PgPool, Postgres, Pool};

use actix_web::{
    get, post, App, HttpResponse, HttpServer, Error, web
};

pub mod models;
use models::ftp;

#[actix_rt::main]
async fn main() -> std::io::Result<()> {

    dotenv::dotenv().ok();
    let backendADDR: String = std::env::var("BACKEND_ADDR").unwrap();
    let databaseURL: String = std::env::var("DATABASE_URL").unwrap();

    let clientSocket: web::Data<Mutex<Option<TcpStream>>> = web::Data::new(Mutex::new(None));
    let pool: Pool<Postgres> = PgPool::connect(&databaseURL).await.unwrap();

    HttpServer::new(move || {
        App::new()
        .wrap(Cors::permissive())
        .app_data(clientSocket.clone())
        .app_data(web::Data::new(pool.clone()))
        .service(ftp::connect)    // POST /connect
        .service(ftp::pwd)        // GET  /pwd
        .service(ftp::cwd)        // POST /cwd
        .service(ftp::cdup)       // POST /cdup
        .service(ftp::list)       // GET  /list
        .service(ftp::mlsd)       // GET  /mlsd
        .service(ftp::stor)       // POST /stor
        .service(ftp::retr)       // POST /retr
        .service(ftp::breakpoint) // POST /breakpoint
    })
    .bind(&backendADDR)?
    .run()
    .await
}

