use actix_web::client::Client;
use actix_web::{get, web, App, HttpServer, Responder};
use futures::future::FutureExt;
use futures::{pin_mut, select};
use tokio::stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

#[get("/{something}")]
async fn index(info: web::Path<String>) -> impl Responder {
    format!("Hello Got this: {}", info)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    println!("Type something and hit enter!");
    let stdin_fut = async {
        let file = tokio_file_unix::raw_stdin().unwrap();
        let file = tokio_file_unix::File::new_nb(file).unwrap();
        let file = file.into_io().unwrap();

        let client = Client::default();

        let mut framed = FramedRead::new(file, LinesCodec::new());

        while let Some(got) = framed.next().await {
            println!("Sending this: {:?}", got);

            let mut response = match client
                .get(format!("http://127.0.0.1:8080/{}", got.unwrap()))
                .send()
                .await
            {
                Err(e) => panic!("{:?}", e),
                Ok(t) => t,
            };

            let body = response.body().await.unwrap();

            println!("Got bytes: {:?}", String::from_utf8(body.to_vec()).unwrap());
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
