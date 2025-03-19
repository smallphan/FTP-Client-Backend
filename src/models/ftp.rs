use actix_cors::Cors;
use actix_rt::time;
use actix_web::cookie::time::macros::offset;
use serde::de;
use sqlx::pool;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, AsyncSeekExt};
use tokio::net::TcpStream;
use tokio::fs::File;
use std::sync::Arc;
use tokio::sync::Mutex;
use sqlx::{PgPool, Postgres, Pool};

use tokio::time::{timeout, Duration};

use actix_web::{
    get, post, App, HttpResponse, HttpServer, Error, web::{self}
};

use super::sql;

const CHUNK_SIZE: usize = 32 * 1024; // 32KB

async fn send_message(
    socket: &mut TcpStream,
    command: String
) -> Result<(), Error> {

    println!("Sending: {}", command);
    socket.write_all((command + "\r\n").as_bytes()).await?;

    Ok(())
}

async fn recv_message(
    socket: &mut TcpStream
) -> Result<String, Error> {

    let mut buffer = [0; CHUNK_SIZE];

    println!("Waiting for response...");
    let result = timeout(Duration::from_millis(500), socket.read(&mut buffer)).await;
    match result {
        Ok(size) => {
            let string = String::from_utf8_lossy(&buffer[..size?]).to_string();
            println!("Received: {}", string);
            Ok(string)
        },
        Err(_) => {
            println!("Timeout\n");
            Ok("Timeout".to_string())
        }
    }
}

async fn send_rawdata(
    socket: &mut TcpStream,
    data: &mut Vec<u8>
) -> Result<(), Error> {

    socket.write_all(&data).await?;

    Ok(())
}

async fn recv_rawdata(
    socket: &mut TcpStream,
    data: &mut Vec<u8>
) -> Result<(), Error> {

    let mut buffer = [0; CHUNK_SIZE];
    let size = socket.read(&mut buffer).await?;
    data.extend_from_slice(&buffer[..size]);

    Ok(())
}

async fn pasv_setting(
    socket: &mut TcpStream
) -> Result<(String, u16), Error> {

    send_message(socket, format!("PASV")).await?; // Send the PASV command
    let response = recv_message(socket).await?; // Receive the response from the PASV command

    let start = response.find('(').unwrap() + 1;
    let end = response.find(')').unwrap();
    let data = &response[start..end];
    let parts: Vec<u16> = data.split(',')
    .map(|s| s.parse().unwrap())
    .collect();

    let ip = format!("{}.{}.{}.{}", parts[0], parts[1], parts[2], parts[3]);
    let port = parts[4] * 256 + parts[5];

    Ok((ip, port))
}

#[derive(serde::Deserialize)]
struct ConnectRequest {
    host: String,
    port: u16,
    username: String,
    password: String
}

#[post("/connect")]
async fn connect(
    clientSocket: web::Data<Mutex<Option<TcpStream>>>,
    connectInfo: web::Json<ConnectRequest>
) -> Result<HttpResponse, Error> {

    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;
    *clientSocket = Some(TcpStream::connect(format!("{}:{}", connectInfo.host, connectInfo.port)).await?);

    if let Some(ref mut clientSocket) = *clientSocket {
        recv_message(clientSocket).await?; // Receive the connection message (Server Information)
        send_message(clientSocket, format!("USER {}", connectInfo.username)).await?; // Send the USER command
        recv_message(clientSocket).await?; // Receive the response from the USER command
        send_message(clientSocket, format!("PASS {}", connectInfo.password)).await?; // Send the PASS command
        recv_message(clientSocket).await?; // Receive the response from the PASS command
        send_message(clientSocket, format!("TYPE I")).await?; // Send the TYPE I command
        recv_message(clientSocket).await?; // Receive the response from the TYPE I command
    } else {
        return Ok(HttpResponse::BadRequest().body("Failed to connect to the FTP server"));
    }

    Ok(HttpResponse::Ok().body("Successfully connected to the FTP server"))
}

#[get("/pwd")]
async fn pwd(
    clientSocket: web::Data<Mutex<Option<TcpStream>>>
) -> Result<HttpResponse, Error> {

    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;
    let mut pwdMessage: String = String::new();

    if let Some(ref mut clientSocket) = *clientSocket {
        send_message(clientSocket, format!("PWD")).await?; // Send the PWD command
        pwdMessage = recv_message(clientSocket).await?; // Receive the response from the PWD command
    } else {
        return Ok(HttpResponse::BadRequest().body("Please connect to the FTP server first"));
    }

    Ok(HttpResponse::Ok().body(format!("Successfully fetched pwd: {}", pwdMessage)))
}

#[derive(serde::Deserialize)]
struct CwdRequest {
    path: String
}

#[post("/cwd")]
async fn cwd(
    clientSocket: web::Data<Mutex<Option<TcpStream>>>,
    cwdInfo: web::Json<CwdRequest>
) -> Result<HttpResponse, Error> {
    
    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;
    let mut cwdMessage: String = String::new();

    if let Some(ref mut clientSocket) = *clientSocket {
        send_message(clientSocket, format!("CWD {}", cwdInfo.path)).await?; // Send the CWD command
        cwdMessage = recv_message(clientSocket).await?; // Receive the response from the CWD command
    } else {
        return Ok(HttpResponse::BadRequest().body("Please connect to the FTP server first"));
    }

    Ok(HttpResponse::Ok().body(format!("Successfully changed the working directory: {}", cwdMessage)))
}

#[post("/cdup")]
async fn cdup(
    clientSocket: web::Data<Mutex<Option<TcpStream>>>
) -> Result<HttpResponse, Error> {
    
    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;
    let mut cdupMessage: String = String::new();

    if let Some(ref mut clientSocket) = *clientSocket {
        send_message(clientSocket, format!("CDUP")).await?; // Send the CDUP command
        cdupMessage = recv_message(clientSocket).await?; // Receive the response from the CDUP command
    } else {
        return Ok(HttpResponse::BadRequest().body("Please connect to the FTP server first"));
    }

    Ok(HttpResponse::Ok().body(format!("Successfully changed the working directory: {}", cdupMessage)))
}

#[get("/list")]
async fn list(
    clientSocket: web::Data<Mutex<Option<TcpStream>>>
) -> Result<HttpResponse, Error> {

    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;
    let mut listMessage: String = String::new();

    if let Some(ref mut clientSocket) = *clientSocket {

        let (ip, port) = pasv_setting(clientSocket).await?; // Get the IP and Port for the PASV setting
        let mut dataSocket: TcpStream = TcpStream::connect(format!("{}:{}", ip, port)).await?; // Connect to the data socket
        
        send_message(clientSocket, format!("LIST")).await?; // Send the LIST command
        recv_message(clientSocket).await?; // Receive the response from the LIST command

        listMessage = recv_message(&mut dataSocket).await?; // Receive the list of files
        
        dataSocket.shutdown().await?; // Shutdown the data socket
        drop(dataSocket); // Close the data socket

        recv_message(clientSocket).await?; // Receive the response from the LIST command
    } else {
        return Ok(HttpResponse::BadRequest().body("Please connect to the FTP server first"));
    }
    
    Ok(HttpResponse::Ok().body(format!("Successfully fetched list: \n{}", listMessage)))
}

#[get("/mlsd")]
async fn mlsd(
    clientSocket: web::Data<Mutex<Option<TcpStream>>>
) -> Result<HttpResponse, Error> {

    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;
    let mut mlsdMessage: String = String::new();

    if let Some(ref mut clientSocket) = *clientSocket {

        let (ip, port) = pasv_setting(clientSocket).await?; // Get the IP and Port for the PASV setting
        let mut dataSocket: TcpStream = TcpStream::connect(format!("{}:{}", ip, port)).await?; // Connect to the data socket

        send_message(clientSocket, format!("MLSD")).await?; // Send the MSLD command
        recv_message(clientSocket).await?; // Receive the response from the MLSD command

        mlsdMessage = recv_message(&mut dataSocket).await?; // Receive the msld of files

        dataSocket.shutdown().await?; // Shutdown the data socket
        drop(dataSocket); // Close the data socket

        recv_message(clientSocket).await?; // Receive the response from the MLSD command
    } else {
        return Ok(HttpResponse::BadRequest().body("Please connect to the FTP server first"));
    }
    
    Ok(HttpResponse::Ok().body(format!("Successfully fetched mlsd: \n{}", mlsdMessage)))
}

#[derive(serde::Deserialize)]
struct STORrequest {
    username: String,
    filepath: String,
    filename: String,
    isResume: bool,
    breakpnt: i64
}

#[post("/stor")]
async fn stor(
    pool: web::Data<PgPool>,
    clientSocket: web::Data<Mutex<Option<TcpStream>>>,
    fileInfo: web::Json<STORrequest>
) -> Result<HttpResponse, Error> {

    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;

    if let Some(ref mut clientSocket) = *clientSocket {

        let mut file: File = File::open(&fileInfo.filepath).await?;
        let fileSize: u64 = file.metadata().await?.len();

        send_message(clientSocket, format!("PWD")).await?; // Send the PWD command
        let pwdMessage: String = recv_message(clientSocket).await?; // Receive the response from the PWD command
        let pwdMessage: String = pwdMessage.trim().split_whitespace().nth(1).unwrap_or("/").trim_matches('"').to_string();

        sql::insert(
            pool.get_ref(), "stor", &fileInfo.username, &fileInfo.filepath, 
            &fileInfo.filename, &pwdMessage, &fileInfo.filename, 0
        ).await?; // Insert the breakpoint

        let (ip, port) = pasv_setting(clientSocket).await?; // Get the IP and Port for the PASV setting
        let mut dataSocket: TcpStream = TcpStream::connect(format!("{}:{}", ip, port)).await?; // Connect to the data socket

        let mut buffer: Vec<u8> = vec![0; CHUNK_SIZE];
        let mut offset: usize = 0;

        if fileInfo.isResume {
            offset = fileInfo.breakpnt as usize;
            file.seek(tokio::io::SeekFrom::Start(offset as u64)).await?; // Seek to the breakpoint
            send_message(clientSocket, format!("REST {}", offset)).await?; // Send the REST command
            recv_message(clientSocket).await?; // Receive the response from the REST command
        }

        send_message(clientSocket, format!("STOR {}", fileInfo.filename)).await?; // Send the STOR command
        recv_message(clientSocket).await?; // Receive the response from the STOR command

        loop {
            let bytes = file.read(&mut buffer).await?;
            println!("offset: {}, bytes: {}", offset, bytes);
            if bytes == 0 {

                sql::delete(
                    pool.get_ref(), "stor", &fileInfo.username, 
                    &fileInfo.filepath, &fileInfo.filename
                ).await?; // Delete the breakpoint

                break;
            } // EOF
            send_rawdata(&mut dataSocket, &mut buffer[..bytes].to_vec()).await?; // Send the file content

            sql::update(
                pool.get_ref(), "stor", &fileInfo.username, 
                &fileInfo.filepath, &fileInfo.filename, offset as i64
            ).await?; // Update the breakpoint

            offset += bytes as usize;
        }

        dataSocket.shutdown().await?; // Shutdown the data socket
        drop(dataSocket); // Close the data socket
        recv_message(clientSocket).await?; // Receive the response from the STOR command
    } else {
        return Ok(HttpResponse::BadRequest().body("Please connect to the FTP server first"));
    }
    
    Ok(HttpResponse::Ok().body("Successfully stored the file"))
}

#[derive(serde::Deserialize)]
struct RETRrequest {
    username: String,
    filepath: String,
    filename: String
}

#[post("/retr")]
async fn retr(
    pool: web::Data<PgPool>,
    clientSocket: web::Data<Mutex<Option<TcpStream>>>,
    fileInfo: web::Json<RETRrequest>
) -> Result<HttpResponse, Error> {

    let mut clientSocket: tokio::sync::MutexGuard<'_, Option<TcpStream>> = clientSocket.lock().await;

    if let Some(ref mut clientSocket) = *clientSocket {

        println!("Retrieving file: {}", fileInfo.filename);
        println!("File created: {}", fileInfo.filepath);
        let mut file: File = File::create(&fileInfo.filepath).await?;

        send_message(clientSocket, format!("PWD")).await?; // Send the PWD command
        let pwdMessage: String = recv_message(clientSocket).await?; // Receive the response from the PWD command
        let pwdMessage: String = pwdMessage.trim().split_whitespace().nth(1).unwrap_or("/").trim_matches('"').to_string();

        sql::insert(
            pool.get_ref(), "retr", &fileInfo.username, &fileInfo.filepath, 
            &fileInfo.filename, &pwdMessage, &fileInfo.filename, 0
        ).await?; // Insert the breakpoint

        let (ip, port) = pasv_setting(clientSocket).await?; // Get the IP and Port for the PASV setting
        let mut dataSocket: TcpStream = TcpStream::connect(format!("{}:{}", ip, port)).await?; // Connect to the data socket

        let mut offset: usize = 0;

        send_message(clientSocket, format!("RETR {}", fileInfo.filename)).await?; // Send the RETR command
        recv_message(clientSocket).await?; // Receive the response from the RETR command

        loop {
            let mut bytes: Vec<u8> = Vec::new();
            recv_rawdata(&mut dataSocket, &mut bytes).await?;
            println!("offset: {}, bytes: {}", offset, bytes.len());
            if bytes.len() == 0 {

                sql::delete(
                    pool.get_ref(), "retr", &fileInfo.username, 
                    &fileInfo.filepath, &fileInfo.filename
                ).await?; // Delete the breakpoint

                break;
            } // EOF
            file.write_all(&bytes).await?; // Write the file content

            sql::update(
                pool.get_ref(), "retr", &fileInfo.username, 
                &fileInfo.filepath, &fileInfo.filename, offset as i64
            ).await?; // Update the breakpoint

            offset += bytes.len();
        }

        dataSocket.shutdown().await?; // Shutdown the data socket
        drop(dataSocket); // Close the data socket
        recv_message(clientSocket).await?; // Receive the response from the RETR command
    } else {
        return Ok(HttpResponse::BadRequest().body("Please connect to the FTP server first"));
    }
    
    Ok(HttpResponse::Ok().body("Successfully retrieved the file"))
}

#[derive(serde::Serialize)]
pub struct Breakpoint {
    pub mode: String,
    pub local_filepath: String,
    pub local_filename: String,
    pub remote_filedir: String,
    pub remote_filename: String,
    pub bytes: i64
}

#[get("/breakpoint/{username}")]
async fn breakpoint(
    pool: web::Data<PgPool>,
    username: web::Path<String>
) -> Result<HttpResponse, Error> {

    let breakpoint: Vec<Breakpoint> = sql::select(pool.get_ref(), &*username).await?;
    Ok(HttpResponse::Ok().json(breakpoint))
}
