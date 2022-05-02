use std::task::{Context, Poll};
use std::future::Future;
use std::pin::Pin;
use std::convert::Infallible;

#[derive(Clone)]
struct HelloWorld;

impl tower::Service<hyper::Request<hyper::Body>> for HelloWorld {
    type Response = hyper::Response<hyper::Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: hyper::Request<hyper::Body>) -> Self::Future {
        let body = hyper::Body::from("hello, world!\n");
        let resp = hyper::Response::builder()
            .status(200)
            .body(body)
            .expect("Unable to create `hyper::Response` object");

        let fut = async {
            Ok(resp)
        };

        Box::pin(fut)
    }
}

#[shuttle_service::main]
async fn tower() -> Result<HelloWorld, shuttle_service::Error> {
    Ok(HelloWorld)
}
