use std::{
    net::{Ipv4Addr, SocketAddr},
    time::SystemTime,
};

use bytes::Bytes;
use futures::join;
use tokio::net::{TcpListener, TcpStream};

use fastcgi::{
    client::Client,
    record::{ByteSlice, Params, Stderr, Stdout},
    request::Request,
    response::{Response, ResponseBuilder},
    server::Server,
    FastcgiClientError, FastcgiServerError,
};

#[tokio::test]
async fn echo_example() {
    let data = b"Echo this.";

    let (_, response) = join!(server(), client(&data[..]));

    let stdout = response.unwrap();
    let stdout_bytes: &Bytes = stdout.get_stdout().as_ref().unwrap().as_ref();

    assert_eq!(&stdout_bytes[..], &data[..]);
}

// One-shot client
async fn client(data: &'static [u8]) -> Result<Response, FastcgiClientError> {
    let port = 8080;
    let addr = Ipv4Addr::new(127, 0, 0, 1);
    let stream = TcpStream::connect(SocketAddr::new(addr.into(), port))
        .await
        .unwrap();

    let mut client = Client::new(stream);

    let params = Params::builder().server_port(port).server_addr(addr.into());
    let request = Request::builder()
        .keep_conn()
        .params(params)
        .data(data.into(), SystemTime::now())
        .build();

    let response = client.send(request).await?;

    dbg!(&response);

    Ok(response)
}

// One-shot server
async fn server() -> Result<(), FastcgiServerError> {
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

    // Wait for a connection.
    let (socket, _) = listener.accept().await.unwrap();
    let mut server = Server::new(socket);

    server.handle_request(echo_data_over_stdout).await
}

fn echo_data_over_stdout(req: Result<Request, FastcgiServerError>) -> Response {
    dbg!(&req);

    match req {
        Ok(req) => {
            let data = req.get_data().unwrap().byte_slice().unwrap().clone();

            ResponseBuilder::new()
                .stdout(ByteSlice::new(data).map(Stdout).unwrap())
                .app_status(0)
                .build()
        }
        Err(e) => {
            let error_message = format!("Server error: {:?}", e).into();

            ResponseBuilder::new()
                .stderr(ByteSlice::new(error_message).map(Stderr).unwrap())
                .app_status(500)
                .build()
        }
    }
}
