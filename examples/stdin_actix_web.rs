extern crate tokio;
extern crate tokio_file_unix;

use tokio_util::codec::FramedRead;
use tokio_util::codec::LinesCodec;
use crate::tokio::stream::StreamExt;
use actix_web::{get, web, App, HttpServer, Responder};
use actix_web::client::{Client};

#[get("/{something}")]
async fn index(info: web::Path<String>) -> impl Responder {
    format!("Hello Got this: {}", info)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    actix_rt::spawn(async {
        let file = tokio_file_unix::raw_stdin().unwrap();
        let file = tokio_file_unix::File::new_nb(file).unwrap();
        let file = file.into_io().unwrap();

        let client = Client::default();

        let mut framed = FramedRead::new(file, LinesCodec::new());

        while let Some(got) = framed.next().await {
            println!("Sending this: {:?}", got);

            let mut response = match client.get(format!("http://127.0.0.1:8080/{}", got.unwrap())).send().await {
                Err(e) => panic!("{:?}", e),
                Ok(t) => t
            };

            let body = response.body().await.unwrap();

            println!("Got bytes: {:?}", String::from_utf8(body.to_vec()).unwrap());
        }
    });

    HttpServer::new(|| App::new().service(index))
        .bind("127.0.0.1:8080")?
        .start()
        .await
}