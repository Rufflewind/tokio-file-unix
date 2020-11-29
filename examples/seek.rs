use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> io::Result<()> {
    let file = File::create("tests/seek.txt")?;
    file.set_len(0x11)?;
    let mut file = tokio_file_unix::File::new_nb(file)?;

    file.write_all(b"aaaaAAAAaaaaAAAA\n").await?;
    file.as_mut().seek(SeekFrom::Start(8))?;
    file.write_all(&[b'b'; 8]).await?;
    file.as_mut().seek(SeekFrom::Start(2))?;
    file.write_all(&[b'c'; 4]).await?;

    Ok(())
}
