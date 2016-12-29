extern crate egg_mode;
extern crate rand;
extern crate toml;
extern crate rusqlite;
extern crate rustc_serialize;
extern crate r2d2;
extern crate r2d2_sqlite;

use std::io;
use std::io::prelude::*;
use std::fs::File;
use rusqlite::Error;

#[derive(Debug, RustcEncodable, RustcDecodable)]
struct AppConfig {
    consumer_key: String,
    consumer_secret: String,
    db_addr: String,
}

#[derive(RustcEncodable)]
pub struct User {
    pub user_id: i32,
    pub twitter_id: i64,
    pub screenname: String,
    pub name: String,
}

fn access_token<'a> (consumer: &egg_mode::Token, request: &egg_mode::Token,
                 oauth_verifier: String) -> Option<egg_mode::Token<'a>> {
    match egg_mode::access_token(consumer, request, oauth_verifier) {
        Ok((token, id, name)) => {
            println!("id: {}, name: {}", id, name);
            return Some(token);
        }
        Err(e) => {
            println!("{}", e);
            return None;
        }
    }
}

fn establish_resourcepool(db: &str)
    -> r2d2::Pool<r2d2_sqlite::SqliteConnectionManager> {
    let config = r2d2::Config::builder().pool_size(4).build();
    let manager = r2d2_sqlite::SqliteConnectionManager::new(&db);
    return r2d2::Pool::new(config, manager).unwrap();
}

fn generate_authorize_url<'a> (consumer: &egg_mode::Token<'a>) -> (String, egg_mode::Token<'a>) {
    let mut req = egg_mode::request_token(consumer, "").unwrap();
    let url = egg_mode::authenticate_url(&req);
    return (url, req);
}

fn read_consumer_token(config: &str) -> AppConfig {
    let mut file = File::open(config).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    return toml::decode_str(&content).unwrap();
}

fn check_accesstoken<'a>(pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>) -> Option<egg_mode::Token<'a>> {
    let conn = pool.get().unwrap();
    let query = "select access_key, access_secret from access_token";
    let response: Result<(String, String), rusqlite::Error> = conn.query_row(query, &[], |row| (row.get(0), row.get(1)));
    match  response {
        Ok(res) => Some(egg_mode::Token::new(res.0, res.1)),
        _ => None
    }
}

fn main() {
    let config = read_consumer_token("setting.toml");
    let consumer = egg_mode::Token::new(config.consumer_key, config.consumer_secret);
    let pool = establish_resourcepool(&config.db_addr);

    if (check_accesstoken(&pool)) {
    }

    let (url, request) = generate_authorize_url(&consumer);

    println!("url: {}", url);

    let mut verifier = String::new();
    io::stdin().read_line(&mut verifier);

    let access = access_token(&consumer, &request, verifier);
}

