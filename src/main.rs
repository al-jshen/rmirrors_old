use serde::Deserialize;
use std::time::{Instant, Duration};
use futures::future::join_all;
use std::fs::File;
use std::io::prelude::*;
use clap::{App, Arg, ArgMatches, crate_authors, crate_version, crate_description};

#[derive(Deserialize, Debug)]
struct Mirror {
    url: String,
    protocol: String,
    last_sync: Option<String>,
    completion_pct: Option<f64>,
    delay: Option<u64>,
    duration_avg: Option<f64>,
    duration_stddev: Option<f64>,
    score: Option<f64>,
    active: bool,
    country: String,
    country_code: String,
    isos: bool,
    ipv4: bool,
    ipv6: bool,
    details: String,
}

#[derive(Deserialize, Debug)]
struct StatusData {
    cutoff: u32,
    last_check: String,
    num_checks: u16,
    check_frequency: u32,
    urls: Vec<Mirror>,
    version: u16
}

#[derive(Debug)]
struct Ranked {
    url: String,
    score: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let options: ArgMatches = App::new("mirrors")
        .author(crate_authors!())
        .version(crate_version!())
        .about(crate_description!())
        .arg(Arg::with_name("fast")
            .help("fast ranking; ranked independently of your connection speed")
            .short("f"))
        .arg(Arg::with_name("filename")
            .help("name of file to write mirrorlist to")
            .short("save")
            .long("save")
            .takes_value(true))
        .get_matches();
            
    let status_req = reqwest::get("https://www.archlinux.org/mirrors/status/json/")
        .await?
        .text()
        .await?;


    let mut status_data: StatusData = serde_json::from_str(&status_req)?;

    let servers = status_data.urls.iter_mut()
        .filter(|x| &x.protocol == "https" && x.ipv4 && x.active)
        .filter(|x| match x.score {
            Some(_) => true,
            None => false,
        })
        .collect::<Vec<_>>();

    let mut ranked = if !options.is_present("fast") {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()?;
    
        let waiting = servers.iter()
            .map(|x| {
                let details = [&x.url, "core/os/x86_64/core.db.tar.gz"].join("");
                get_response_time(&client, details)
            })
            .collect::<Vec<_>>();

        let times = join_all(waiting).await;

    
        (0..servers.len()).into_iter()
            .filter_map(|i| {
                if let Ok(time) = times[i] {
                    let url = ["Server = ", &servers[i].url, "$repo/os/$arch"].join("");
                    let score = weighted_score(servers[i].score?, (time as f64) / 1000.);
                    Some(Ranked { url, score })
                } else {
                    None
                }
            })
            .filter(|x| x.score > 0.5)
            .collect::<Vec<_>>()
    } else {
        servers.into_iter()
            .map(|x| { 
                Ranked { url: x.url.to_string(), score: 1./x.score.unwrap() }
            })
            .collect::<Vec<_>>()
    };
    
    ranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());


    if let Some(filename) = options.value_of("filename") {
        let strings = ranked.iter()
            .map(|x| x.url.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut file = File::create(filename)?;
        file.write_all(strings.as_bytes())?;
    } else {
        ranked.iter()
            .for_each(|mirror| {
                println!("{}", &mirror.url);
            })
    }

    Ok(())
}

async fn get_response_time(client: &reqwest::Client, url: String) -> Result<u128, Box<dyn std::error::Error>> {
    let now = Instant::now();
    client.get(&url).send().await?;
    Ok(now.elapsed().as_millis())
}

fn weighted_score(score: f64, time: f64) -> f64 { // probably better to find something less expensive? nvm rust is so fast it doesnt matter.
    (-(time * time) / 100.).exp() * 0.5 +
    (-(score * score) / 100.).exp() * 0.5
}
