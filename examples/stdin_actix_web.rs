use actix_web::client::Client;
use actix_web::{get, web, App, HttpServer, Responder};
use futures::future::FutureExt;
use futures::{pin_mut, select};
use std::{error, io};
use tokio::stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

fn stringify_error<E: error::Error>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

#[get("/{something}")]
async fn index(info: web::Path<String>) -> impl Responder {
    format!("Hello Got this: {}", info)
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    println!("Type something and hit enter!");
    let stdin_fut = async {
        let file = tokio_file_unix::raw_stdin()?;
        let file = tokio_file_unix::File::new_nb(file)?;
        let file = file.into_io()?;

        let client = Client::default();

        let mut framed = FramedRead::new(file, LinesCodec::new());

        while let Some(got) = framed.next().await {
            println!("Sending this: {:?}", got);

            let mut response = client
                .get(format!(
                    "http://127.0.0.1:8080/{}",
                    got.map_err(stringify_error)?
                ))
                .send()
                .await
                .map_err(stringify_error)?;

            let body = response.body().await.map_err(stringify_error)?;

            println!(
                "Got bytes: {:?}",
                String::from_utf8(body.to_vec()).map_err(stringify_error)?,
            );
        }
        Ok(())
    }
    .fuse();

    let server_fut = HttpServer::new(|| App::new().service(index))
        .bind("127.0.0.1:8080")?
        .run()
        .fuse();

    pin_mut!(stdin_fut, server_fut);
    select! {
        result = stdin_fut => result,
        result = server_fut => result,
    }
}
