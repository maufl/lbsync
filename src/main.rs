extern crate chrono;
extern crate clap;
extern crate reqwest;
extern crate scraper;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate uuid;

use clap::{App, Arg};

#[derive(Serialize, Deserialize)]
struct Data {
    local_account_number: String,
    amount: String,
    currency: String,
    timestamp: i64,
}

fn main() {
    let matches = app().get_matches();
    let id = matches.value_of("id").unwrap();
    let pin = matches.value_of("pin").unwrap();
    let state = matches.value_of("state").unwrap();
    let (amount, currency) = match fetch_balance(id, pin, state) {
        Ok(r) => r,
        Err(err) => return println!("Unable to fetch balance: {}", err),
    };
    let local_account_number = matches.value_of("account_number").unwrap();
    let now = chrono::Utc::now();
    let data = Data {
        local_account_number: local_account_number.to_owned(),
        amount: amount,
        currency: currency,
        timestamp: now.timestamp(),
    };
    let host = matches.value_of("database_host").unwrap();
    let port = matches.value_of("database_port").unwrap();
    let database_name = matches.value_of("database_name").unwrap();
    if matches.value_of("database_type").unwrap() == "influxdb" {
        if let Err(err) = post_data_to_influxdb(host, port, database_name, &data) {
            println!("Error posting data to InfluxDB: {}", err)
        }
    } else {
        if let Err(err) = post_data_to_couchdb(host, port, database_name, &data) {
            println!("Error posting data to CouchDB: {}", err)
        }
    }
}

fn fetch_balance(id: &str, pin: &str, state: &str) -> Result<(String, String), String> {
    let params = [("REQ_ID", "LOGIN"), ("IN_ID", id), ("IN_PIN", pin)];
    let url = format!("https://kundenservice.lbs.de/lbs-{}/guiServlet", state);
    let client = reqwest::Client::new();
    let mut resp = client
        .post(&url)
        .form(&params)
        .send()
        .map_err(|err| format!("Fetching balance failed: {}", err))?;
    let text = resp.text()
        .map_err(|err| format!("Failed to get response html: {}", err))?;
    let html = scraper::Html::parse_document(&text);
    let selector = scraper::Selector::parse("#rechner_tarif_details table tr.odd td").unwrap();
    let balance = html.select(&selector)
        .nth(3)
        .ok_or_else(|| "Field with balance not found")?
        .inner_html();
    let amount = balance
        .split_whitespace()
        .next()
        .ok_or_else(|| "Balance field has unexpected format")?
        .replace(".", "")
        .replace(",", ".");
    let currency = balance
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "Balance field has unexpected format")?;
    Ok((amount.to_owned(), currency.to_owned()))
}

fn post_data_to_influxdb(
    host: &str,
    port: &str,
    database: &str,
    data: &Data,
) -> Result<(), String> {
    let payload = format!(
        "balance,localAccountNumber={} balance={},currency=\"{}\" {}000000000",
        data.local_account_number, data.amount, data.currency, data.timestamp
    );
    let url = format!("http://{}:{}/write?db={}", host, port, database);
    let client = reqwest::Client::new();
    client
        .post(&url)
        .body(payload.to_owned())
        .send()
        .and_then(|mut resp| {
            print!("{}", resp.text().unwrap());
            if resp.status().is_success() {
                Ok(())
            } else {
                resp.error_for_status().map(|_| ())
            }
        })
        .map_err(|err| format!("Failed to POST to influxdb: {}", err))
}

fn post_data_to_couchdb(host: &str, port: &str, database: &str, data: &Data) -> Result<(), String> {
    let uuid = uuid::Uuid::new_v4();
    let url = format!("http://{}:{}/{}/{}", host, port, database, uuid);
    let client = reqwest::Client::new();
    client
        .put(&url)
        .body(serde_json::to_string(data).unwrap())
        .send()
        .and_then(|mut resp| {
            print!("{}", resp.text().unwrap());
            if resp.status().is_success() {
                Ok(())
            } else {
                resp.error_for_status().map(|_| ())
            }
        })
        .map_err(|err| format!("Failed to PUT new document into couchdb: {}", err))
}

fn app() -> App<'static, 'static> {
    App::new("lbsync")
        .version("0.1")
        .author("Felix Konstantin Maurer <github@maufl.de>")
        .about("Scrape balance of a Bausparvertrag and write it to InfluxDB")
        .arg(
            Arg::with_name("account_number")
                .short("a")
                .long("account_number")
                .help("The account number for which to write it to the InfluxDB.")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("id")
                .short("i")
                .long("id")
                .help("Your online ID to log in.")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("pin")
                .short("P")
                .long("pin")
                .help("Your pin to log in.")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("state")
                .short("s")
                .long("state")
                .help("The state of the LBS, i.e. bw, nw.")
                .takes_value(true)
                .required(true)
                .possible_values(&["bw", "nw"]),
        )
        .arg(
            Arg::with_name("database_type")
                .short("t")
                .long("database_type")
                .help("The type of database to use.")
                .takes_value(true)
                .default_value("influxdb")
                .possible_values(&["influxdb", "couchdb"]),
        )
        .arg(
            Arg::with_name("database_host")
                .short("h")
                .long("host")
                .help("The host name of the database.")
                .takes_value(true)
                .default_value("localhost"),
        )
        .arg(
            Arg::with_name("database_port")
                .short("p")
                .long("port")
                .help("The port of the database.")
                .takes_value(true)
                .required(true)
                .default_value_ifs(&[
                    ("database_type", Some("influxdb"), "8086"),
                    ("database_type", Some("couchdb"), "5984"),
                ]),
        )
        .arg(
            Arg::with_name("database_name")
                .short("d")
                .long("database")
                .help("The database name to write the values to.")
                .takes_value(true)
                .required(true),
        )
}
