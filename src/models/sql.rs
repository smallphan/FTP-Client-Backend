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
    get, post, App, HttpResponse, HttpServer, Error
};

use super::ftp::Breakpoint;

pub async fn insert(
    pool: &PgPool,
    mode: &str,
    username: &str,
    local_filepath: &str,
    local_filename: &str,
    remote_filedir: &str,
    remote_filename: &str,
    bytes: i64
) -> Result<(), Error> {

    sqlx::query!(
        "
        INSERT INTO breakpoint 
        (mode, username, local_filepath, local_filename, remote_filedir, remote_filename, bytes)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ",
        mode,
        username,
        local_filepath,
        local_filename,
        remote_filedir,
        remote_filename,
        bytes
    )
    .execute(pool)
    .await
    .map_err(|err| {
        println!("Database error: {:?}", err);
        actix_web::error::ErrorInternalServerError(format!("Failed to insert database.\nDatabase error: {}", err))
    })?; // Insert the breakpoint

    Ok(())
}

pub async fn update(
    pool: &PgPool,
    mode: &str,
    username: &str,
    local_filepath: &str,
    local_filename: &str,
    bytes: i64
) -> Result<(), Error> {

    sqlx::query!(
        "
        UPDATE breakpoint
        SET bytes = $1
        WHERE mode = $2 AND username = $3 AND local_filepath = $4 AND local_filename = $5
        ",
        bytes,
        mode,
        username,
        local_filepath,
        local_filename,
    )
    .execute(pool)
    .await
    .map_err(|err| {
        println!("Database error: {:?}", err);
        actix_web::error::ErrorInternalServerError(format!("Failed to update database.\nDatabase error: {}", err))
    })?; // Update the breakpoint

    Ok(())
}

pub async fn delete(
    pool: &PgPool,
    mode: &str,
    username: &str,
    local_filepath: &str,
    local_filename: &str,
) -> Result<(), Error> {

    sqlx::query!(
        "
        DELETE FROM breakpoint
        WHERE mode = $1 AND username = $2 AND local_filepath = $3 AND local_filename = $4
        ",
        mode,
        username,
        local_filepath,
        local_filename,
    )
    .execute(pool)
    .await
    .map_err(|err| {
        println!("Database error: {:?}", err);
        actix_web::error::ErrorInternalServerError(format!("Failed to delete database.\nDatabase error: {}", err))
    })?; // Delete the breakpoint

    Ok(())
}

pub async fn select(
    pool: &PgPool,
    username: &str,
) -> Result<Vec<Breakpoint>, Error> {

    let row = sqlx::query!(
        "
        SELECT mode, local_filepath, local_filename, remote_filedir, remote_filename, bytes
        FROM breakpoint
        WHERE username = $1 AND mode = 'stor'
        ",
        username,
    )
    .fetch_all(pool)
    .await
    .map_err(|err| {
        println!("Database error: {:?}", err);
        actix_web::error::ErrorInternalServerError(format!("Failed to select database.\nDatabase error: {}", err))
    })?; // Select the breakpoint

    let row: Vec<Breakpoint> = row.into_iter().map(|row| {
        Breakpoint {
            mode:            row.mode           .unwrap_or_default(),
            local_filepath:  row.local_filepath .unwrap_or_default(),
            local_filename:  row.local_filename .unwrap_or_default(),
            remote_filedir:  row.remote_filedir .unwrap_or_default(),
            remote_filename: row.remote_filename.unwrap_or_default(),
            bytes:           row.bytes          .unwrap_or_default()
        }
    }).collect();

    Ok(row)
}