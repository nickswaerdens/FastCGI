pub mod client;
pub mod codec;
pub mod conn;
pub mod meta;
pub mod record;
pub mod request;
pub mod response;
pub mod server;

pub const FCGI_VERSION_1: u8 = 1;

pub const MANAGEMENT_ID: u16 = 0;

mod tests {
    use std::fs::File;

    use tokio::net::{TcpListener, TcpStream};

    use crate::{
        client::Client,
        record::{begin_request::Role, ByteSlice, Data, Kind, Stdout},
        request::Request,
        response::Response,
        server::Server,
    };

    #[tokio::test]
    async fn client() {
        let stream = TcpStream::connect("127.0.0.1:8080").await.unwrap();

        let mut client = Client::new(stream);

        let file = File::open("tests/echo.txt").unwrap();
        let data = Data::new_reader(file);

        let request = Request {
            role: Some(Role::Filter),
            params: None,
            stdin: None,
            data: Some(data),
        };

        client.send_request(request).await.unwrap();

        let response = client.recv_response().await;

        dbg!(response);
    }

    #[tokio::test]
    async fn server_echo_data() {
        let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

        let (socket, _) = listener.accept().await.unwrap();

        let mut server = Server::new(socket);

        let request = server.recv_request().await;

        match request {
            Ok(Some(request)) => {
                dbg!(&request);

                let data = request.data.unwrap();

                let bytes = match data.kind {
                    Kind::ByteSlice(bytes) => bytes,
                    _ => unreachable!(),
                };

                let response = Response {
                    app_status: Some(1),
                    stdout: ByteSlice::new(bytes).map(Stdout),
                    stderr: None,
                };

                server.send_response(response).await.unwrap();
            }
            Ok(None) => {
                dbg!("Request was aborted.");
            }
            Err(e) => {
                dbg!("Encountered an error:", e);
            }
        }
    }
}
