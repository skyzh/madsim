// remove this after prost-build fix clippy issue
#![allow(clippy::derive_partial_eq_without_eq)]

use std::pin::Pin;
use std::time::Duration;

use async_stream::try_stream;
use futures_core::Stream;
use madsim::time::sleep;
use tonic::{transport::Server, Request, Response, Status, Streaming};

use hello_world::another_greeter_server::{AnotherGreeter, AnotherGreeterServer};
use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};

pub mod hello_world {
    tonic::include_proto!("helloworld");
}

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl AnotherGreeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);
        let reply = HelloReply {
            message: format!("Hi {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);
        let remote_addr = request.remote_addr().expect("no remote address");
        let name = request.into_inner().name;
        if name == "error" {
            return Err(Status::invalid_argument("error!"));
        }
        let reply = HelloReply {
            message: format!("Hello {}! ({})", name, remote_addr.ip()),
        };
        Ok(Response::new(reply))
    }

    type LotsOfRepliesStream = Pin<Box<dyn Stream<Item = Result<HelloReply, Status>> + Send>>;

    async fn lots_of_replies(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<Self::LotsOfRepliesStream>, Status> {
        println!("Got a request: {:?}", request);
        let remote_addr = request.remote_addr().expect("no remote address");
        let stream = try_stream! {
            let name = request.into_inner().name;
            for i in 0..3 {
                yield HelloReply {
                    message: format!("{i}: Hello {name}! ({})", remote_addr.ip()),
                };
                sleep(Duration::from_secs(1)).await;
            }
            Err(Status::unknown("EOF"))?;
        };
        Ok(Response::new(Box::pin(stream)))
    }

    async fn lots_of_greetings(
        &self,
        request: Request<Streaming<HelloRequest>>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);
        let remote_addr = request.remote_addr().expect("no remote address");
        let mut stream = request.into_inner();
        let mut s = String::new();
        while let Some(request) = stream.message().await? {
            println!("-> {:?}", request);
            s += " ";
            s += &request.name;
        }
        let reply = HelloReply {
            message: format!("Hello{s}! ({})", remote_addr.ip()),
        };
        Ok(Response::new(reply))
    }

    type BidiHelloStream = Pin<Box<dyn Stream<Item = Result<HelloReply, Status>> + Send>>;

    async fn bidi_hello(
        &self,
        request: Request<Streaming<HelloRequest>>,
    ) -> Result<Response<Self::BidiHelloStream>, Status> {
        println!("Got a request: {:?}", request);
        let remote_addr = request.remote_addr().expect("no remote address");
        let stream = try_stream! {
            let mut stream = request.into_inner();
            while let Some(request) = stream.message().await? {
                println!("-> {:?}", request);
                yield HelloReply {
                    message: format!("Hello {}! ({})", request.name, remote_addr.ip()),
                };
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;

    Server::builder()
        .add_service(GreeterServer::new(MyGreeter::default()))
        .add_service(AnotherGreeterServer::new(MyGreeter::default()))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
#[cfg(madsim)]
mod tests {
    use super::hello_world::another_greeter_client::AnotherGreeterClient;
    use super::hello_world::greeter_client::GreeterClient;
    use async_stream::stream;
    use madsim::{
        rand::{thread_rng, Rng},
        runtime::Handle,
        time::sleep,
    };
    use std::net::SocketAddr;

    use super::*;

    #[madsim::test]
    async fn basic() {
        let handle = Handle::current();
        let addr0 = "10.0.0.1:50051".parse::<SocketAddr>().unwrap();
        let ip1 = "10.0.0.2".parse().unwrap();
        let ip2 = "10.0.0.3".parse().unwrap();
        let ip3 = "10.0.0.4".parse().unwrap();
        let ip4 = "10.0.0.5".parse().unwrap();
        let ip5 = "10.0.0.6".parse().unwrap();
        let node0 = handle.create_node().name("server").ip(addr0.ip()).build();
        let node1 = handle.create_node().name("client1").ip(ip1).build();
        let node2 = handle.create_node().name("client2").ip(ip2).build();
        let node3 = handle.create_node().name("client3").ip(ip3).build();
        let node4 = handle.create_node().name("client4").ip(ip4).build();
        let node5 = handle.create_node().name("client5").ip(ip5).build();

        node0.spawn(async move {
            Server::builder()
                .add_service(GreeterServer::new(MyGreeter::default()))
                .add_service(AnotherGreeterServer::new(MyGreeter::default()))
                .serve(addr0)
                .await
                .unwrap();
        });

        // unary
        let task1 = node1.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let request = tonic::Request::new(HelloRequest {
                name: "Tonic".into(),
            });
            let response = client.say_hello(request).await.unwrap();
            assert_eq!(response.into_inner().message, "Hello Tonic! (10.0.0.2)");

            let request = tonic::Request::new(HelloRequest {
                name: "error".into(),
            });
            let response = client.say_hello(request).await.unwrap_err();
            assert_eq!(response.code(), tonic::Code::InvalidArgument);
        });

        // another service
        let task2 = node2.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = AnotherGreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let request = tonic::Request::new(HelloRequest {
                name: "Tonic".into(),
            });
            let response = client.say_hello(request).await.unwrap();
            assert_eq!(response.into_inner().message, "Hi Tonic!");
        });

        // server stream
        let task3 = node3.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let request = tonic::Request::new(HelloRequest {
                name: "Tonic".into(),
            });
            let response = client.lots_of_replies(request).await.unwrap();
            let mut stream = response.into_inner();
            for i in 0..3 {
                let reply = stream.message().await.unwrap().unwrap();
                assert_eq!(reply.message, format!("{i}: Hello Tonic! (10.0.0.4)"));
            }
            let error = stream.message().await.unwrap_err();
            assert_eq!(error.code(), tonic::Code::Unknown);
        });

        // client stream
        let task4 = node4.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let response = client.lots_of_greetings(hello_stream()).await.unwrap();
            assert_eq!(
                response.into_inner().message,
                "Hello Tonic0 Tonic1 Tonic2! (10.0.0.5)"
            );
        });

        // bi-directional stream
        let task5 = node5.spawn(async move {
            sleep(Duration::from_secs(1)).await;
            let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap();
            let response = client.bidi_hello(hello_stream()).await.unwrap();
            let mut stream = response.into_inner();
            let mut i = 0;
            while let Some(reply) = stream.message().await.unwrap() {
                assert_eq!(reply.message, format!("Hello Tonic{i}! (10.0.0.6)"));
                i += 1;
            }
            assert_eq!(i, 3);
        });

        task1.await.unwrap();
        task2.await.unwrap();
        task3.await.unwrap();
        task4.await.unwrap();
        task5.await.unwrap();
    }

    fn hello_stream() -> impl Stream<Item = HelloRequest> {
        stream! {
            for i in 0..3 {
                yield HelloRequest {
                    name: format!("Tonic{i}"),
                };
                sleep(Duration::from_secs(1)).await;
            }
        }
    }

    #[madsim::test]
    async fn invalid_address() {
        let handle = Handle::current();
        let ip1 = "10.0.0.2".parse().unwrap();
        let node1 = handle.create_node().name("client").ip(ip1).build();

        let task1 = node1.spawn(async move {
            GreeterClient::connect("http://10.0.0.1:50051")
                .await
                .unwrap_err();
        });
        task1.await.unwrap();
    }

    // crash client and see whether server works as well
    #[madsim::test]
    async fn client_crash() {
        let handle = Handle::current();
        let addr0 = "10.0.0.1:50051".parse::<SocketAddr>().unwrap();
        let ip1 = "10.0.0.2".parse().unwrap();
        let node0 = handle.create_node().name("server").ip(addr0.ip()).build();
        node0.spawn(async move {
            Server::builder()
                .add_service(GreeterServer::new(MyGreeter::default()))
                .serve(addr0)
                .await
                .unwrap();
        });
        sleep(Duration::from_secs(1)).await;

        let node1 = handle
            .create_node()
            .name("client1")
            .ip(ip1)
            .init(|| async move {
                let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                    .await
                    .unwrap();
                loop {
                    // initiate a bidi stream
                    let response = client.bidi_hello(hello_stream()).await.unwrap();
                    let mut stream = response.into_inner();
                    sleep(Duration::from_secs(1)).await;

                    // unary
                    let request = tonic::Request::new(HelloRequest {
                        name: "Tonic".into(),
                    });
                    let response = client.say_hello(request).await.unwrap();
                    assert_eq!(response.into_inner().message, "Hello Tonic! (10.0.0.2)");

                    let mut i = 0;
                    while let Some(reply) = stream.message().await.unwrap() {
                        assert_eq!(reply.message, format!("Hello Tonic{i}! (10.0.0.2)"));
                        i += 1;
                    }
                    assert_eq!(i, 3);
                }
            })
            .build();

        for _ in 0..10 {
            sleep(thread_rng().gen_range(Duration::default()..Duration::from_secs(5))).await;
            handle.restart(node1.id());
        }
    }

    #[madsim::test]
    async fn client_drops_response_stream() {
        let handle = Handle::current();
        let addr0 = "10.0.0.1:50051".parse::<SocketAddr>().unwrap();
        let ip1 = "10.0.0.2".parse().unwrap();
        let node0 = handle.create_node().name("server").ip(addr0.ip()).build();
        node0.spawn(async move {
            Server::builder()
                .add_service(GreeterServer::new(MyGreeter::default()))
                .serve(addr0)
                .await
                .unwrap();
        });
        sleep(Duration::from_secs(1)).await;

        handle
            .create_node()
            .name("client1")
            .ip(ip1)
            .build()
            .spawn(async move {
                let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                    .await
                    .unwrap();
                let request = tonic::Request::new(HelloRequest {
                    name: "Tonic".into(),
                });
                let _ = client.lots_of_replies(request).await.unwrap();
                // ^ drop response stream
                sleep(Duration::from_secs(10)).await;
            })
            .await
            .unwrap();
    }

    #[madsim::test]
    async fn server_crash() {
        let handle = Handle::current();
        let addr0 = "10.0.0.1:50051".parse::<SocketAddr>().unwrap();
        let ip1 = "10.0.0.2".parse().unwrap();
        let node0 = handle.create_node().name("server").ip(addr0.ip()).build();
        node0.spawn(async move {
            Server::builder()
                .add_service(GreeterServer::new(MyGreeter::default()))
                .serve(addr0)
                .await
                .unwrap();
        });
        sleep(Duration::from_secs(1)).await;

        let node1 = handle.create_node().name("client1").ip(ip1).build();
        node1
            .spawn(async move {
                let mut client = GreeterClient::connect("http://10.0.0.1:50051")
                    .await
                    .unwrap();
                let request = tonic::Request::new(HelloRequest {
                    name: "Tonic".into(),
                });
                client.say_hello(request).await.unwrap();

                Handle::current().kill(node0.id());

                let request = tonic::Request::new(HelloRequest {
                    name: "Tonic".into(),
                });
                let error = client.say_hello(request).await.unwrap_err();
                assert_eq!(error.code(), tonic::Code::Unavailable);
            })
            .await
            .unwrap();
    }
}
