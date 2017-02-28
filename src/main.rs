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
use std::env;

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

#[derive(RustcEncodable, RustcDecodable)]
struct FollowEvent {
    id: i64,
	user_id: i64,
	founddate: i64,
	event_type: i64
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
    let req = egg_mode::request_token(consumer, "").unwrap();
    let url = egg_mode::authenticate_url(&req);
    return (url, req);
}

fn read_consumer_token(config: &str) -> AppConfig {
    let mut file = File::open(config).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    return toml::decode_str(&content).unwrap();
}

/* Find previously stored accesstoken */
fn find_accesstoken<'a>(pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>) -> Option<egg_mode::Token<'a>> {
    let conn = pool.get().unwrap();
    let query = "select access_key, access_secret from access_token";
    let response: Result<(String, String), rusqlite::Error> = conn.query_row(query, &[], |row| (row.get(0), row.get(1)));
    match  response {
        Ok(res) => Some(egg_mode::Token::new(res.0, res.1)),
        _ => None
    }
}

/* Store accesstoken into database */
fn store_accesstoken(token: &egg_mode::Token, pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>) {
    let conn = pool.get().unwrap();
    let query = "insert into access_token(access_key, access_secret) values($1, $2)";
    conn.execute(query, &[&token.key.as_ref(), &token.secret.as_ref()]);
}

fn fetch_accesstoken<'a>(consumer: &egg_mode::Token<'a>) -> egg_mode::Token<'a> {
    let (url, request) = generate_authorize_url(&consumer);
    let mut verifier = String::new();

    println!("url: {}", url);
    io::stdin().read_line(&mut verifier);

    return access_token(&consumer, &request, verifier).unwrap();
}

fn check_follower_events<'a>(followers: egg_mode::cursor::CursorIter<'a, egg_mode::cursor::UserCursor>, verbose: bool) {
    if verbose println!("######dump all current followers######");
    for follower in followers.map(|u| {let uu = u.unwrap().response; (uu.screen_name, uu.name)}) {
        if verbose println!("@{}:{}", follower.0, follower.1);
    }
}

fn get_known_followers<'a>(pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>) -> Vec<i64> {
    let conn = pool.get().unwrap();
    let query = "select user_id from follower";
    let follower_list = conn.query_map(query, &[], |row| row.get(0)).unwrap();

}

fn main() {
    let config = read_consumer_token("setting.toml");
    let consumer = egg_mode::Token::new(config.consumer_key, config.consumer_secret);
    let pool = establish_resourcepool(&config.db_addr);
    let verbose = false; //XXX

    let access = match find_accesstoken(&pool) {
        Some(token) => token,
        _   => {
            let token = fetch_accesstoken(&consumer);
            store_accesstoken(&token, &pool);
            token
        }
    };

    let ref cred = egg_mode::verify_tokens(&consumer, &access).unwrap();
    println!("Using this account's token @{}: {}", cred.screen_name, cred.name);

    let current_followers = egg_mode::user::followers_of(&cred.screen_name, &consumer, &access);
    let previous_followers = get_known_followers(&pool);
}

