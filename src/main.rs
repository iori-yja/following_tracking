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
use std::collections::*;
use std::iter::FromIterator;

#[derive(Debug, RustcEncodable, RustcDecodable)]
struct AppConfig {
    consumer_key: String,
    consumer_secret: String,
    db_addr: String,
}

#[derive(Hash, RustcEncodable)]
pub struct User {
    pub id: i64,
    pub twitter_id: i64,
    pub screenname: String,
    pub name: String,
}

impl PartialEq for User {
    fn eq(&self, other: &User) -> bool {
        self.twitter_id == other.twitter_id
    }
}

impl Eq for User {}

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
    let query = "select access_key, access_secret from access_token";
    let conn = pool.get().unwrap();
    let mut stmt = conn.prepare(query).unwrap();
    let response: Result<(String, String), _> = stmt.query_row(&[], |row| (row.get(0), row.get(1)));
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

fn check_diff_lists<'a>(current: egg_mode::cursor::CursorIter<'a, egg_mode::cursor::UserCursor>,
                             mut previous: HashSet<i64>) -> (HashSet<User>, HashSet<i64>) {
    let mut newface = HashSet::new();

    for f in current.map(|u| {u.unwrap()}) {
        if !previous.remove(&f.id) {
            newface.insert(
                User {
                    id: 0,
                    twitter_id: f.id,
                    screenname: f.screen_name.clone(),
                    name: f.name.clone()
                });
        }
    }

    (newface, previous)
}

fn get_known_accounts<'a>(pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>, table: &str) -> HashSet<i64> {
    let query = format!("select user_id from {}", table);
    let conn = pool.get().unwrap();
    let mut stmt = conn.prepare(&query).unwrap();
    let follower_list = stmt.query_map(&[], |row| row.get(0)).unwrap();
    let mut ret = HashSet::new();
    for f in follower_list {
        ret.insert(f.unwrap());
    }
    ret
}

/*
struct FollowEvent {
    id: i64,
	user_id: i64,
	founddate: i64,
	event_type: i64
}
*/
fn store_follower_events(pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>, comes: Vec<User>, leaves: Vec<User>, date: i64) {
    let conn = pool.get().unwrap();
    let query = "insert into follow_event(user_id, founddate, event_type) values($1, $2, $3)";
    let mut t = 1; /* `1` indicates that the event is 'new follower` event */

    for c in comes {
        conn.execute(query, &[&c.id, &date, &t]);
    }

    t = 0;
    for l in leaves {
        conn.execute(query, &[&l.id, &date, &t]);
    }
}

fn store_user_if_not_known(pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>, users: Vec<User>) -> Vec<User> {
    let conn = pool.get().unwrap();
    let query = "insert into users(twitter_id, screenname, name) values($1, $2, $3)";
    let mut ret = Vec::new();

    for mut u in users {
        /* Because of unique restriction, write will fail if they are known. */
        let wrote = conn.execute(query, &[&u.twitter_id, &u.screenname, &u.name]).unwrap();
        if wrote == 1 {
            u.id = conn.last_insert_rowid();
        } else {
            u.id = conn.query_row("select id from users where twitter=$1", &[&u.twitter_id], |row| {row.get(0)}).unwrap();
        }
        ret.push(u);
    }
    ret
}

fn print_follow_event(new: &Vec<User>, rem: &Vec<User>) {
    println!("######### show new follower #########");
    for i in new {
        println!("{} (@{})", i.screenname, i.name);
    }

    println!("######## show who removed me ########");
    for u in rem {
        println!("{} (@{})", u.screenname, u.name);
    }
}

fn main() {
    let config = read_consumer_token("setting.toml");
    let consumer = egg_mode::Token::new(config.consumer_key, config.consumer_secret);
    let pool = establish_resourcepool(&config.db_addr);

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
    let previous_followers = get_known_accounts(&pool, "follower");

    let (n, r) = check_diff_lists(current_followers, previous_followers);

    let converter =
        |x: egg_mode::user::TwitterUser| {
            User {
            id: 0,
            twitter_id: x.id,
            screenname: x.screen_name,
            name: x.name
            }
        };

    let newfaces = store_user_if_not_known(&pool, Vec::from_iter(n));
    let newloosers = Vec::from_iter(
                        egg_mode::user::lookup(&Vec::from_iter(r), &consumer, &access)
                            .unwrap().response
                            .into_iter().map(converter)
                     );

    print_follow_event(&newfaces, &newloosers);
}

