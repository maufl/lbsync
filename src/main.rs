extern crate chrono;
extern crate clap;
extern crate reqwest;
extern crate scraper;

use clap::{App, Arg};

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
    let data = format!(
        "balance,localAccountNumber={} balance={},currency=\"{}\" {}000000000",
        local_account_number,
        amount,
        currency,
        now.timestamp()
    );
    let host = matches.value_of("influx_host").unwrap();
    let port = matches.value_of("influx_port").unwrap();
    let database = matches.value_of("influx_database").unwrap();
    if let Err(err) = post_data_to_influxdb(host, port, database, &data) {
        println!("Error posting data to InfluxDB: {}", err)
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

fn post_data_to_influxdb(host: &str, port: &str, database: &str, data: &str) -> Result<(), String> {
    let url = format!("http://{}:{}/write?db={}", host, port, database);
    let client = reqwest::Client::new();
    let _resp = client
        .post(&url)
        .body(data.to_owned())
        .send()
        .map_err(|err| format!("Failed to POST to influxdb: {}", err))?;
    Ok(())
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
                .required(true),
        )
        .arg(
            Arg::with_name("influx_host")
                .short("h")
                .long("host")
                .help("The host name of the InfluxDB.")
                .takes_value(true)
                .default_value("localhost"),
        )
        .arg(
            Arg::with_name("influx_port")
                .short("p")
                .long("port")
                .help("The port of the InfluxDB.")
                .takes_value(true)
                .default_value("8086"),
        )
        .arg(
            Arg::with_name("influx_database")
                .short("d")
                .long("database")
                .help("The InfluxDB database to write the value to")
                .takes_value(true)
                .required(true),
        )
}
