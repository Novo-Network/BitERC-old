use std::{
    fmt::Debug,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use hyper::{
    body::to_bytes, http::response::Builder, service::Service, Body, Request, Response, Server,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{RPCError, RPCRequest, RPCResponse};

#[async_trait]
pub trait Handle {
    type Request: for<'de> Deserialize<'de> + Send + Sync + Clone + 'static;
    type Response: Serialize + Send;

    async fn handle(
        &self,
        method: &str,
        req: Option<Self::Request>,
    ) -> std::result::Result<Option<Self::Response>, RPCError>;

    async fn batch_handle(
        &self,
        reqests: Vec<RPCRequest<Option<Self::Request>>>,
    ) -> Vec<RPCResponse<Self::Response>> {
        let mut response = vec![];
        for reqest in reqests {
            let resp = self
                .handle(&reqest.method, reqest.params)
                .await
                .map_or_else(
                    |e| RPCResponse::error(reqest.id.clone(), e),
                    |v| RPCResponse::result(reqest.id.clone(), v),
                );
            response.push(resp);
        }
        response
    }
}

async fn _handle<H>(req_body: serde_json::Value, handle: &H) -> Result<serde_json::Value>
where
    H: Handle,
    H::Request: Debug,
{
    let req: RPCRequest<Option<H::Request>> = serde_json::from_value(req_body)?;

    log::info!("Get call method: {}", &req.method);
    log::debug!("Params is: {:?}", &req.params);

    let r = match handle.handle(&req.method, req.params).await {
        Ok(v) => RPCResponse::result(req.id, v),
        Err(e) => RPCResponse::error(req.id, e),
    };

    r.into_value()
}
async fn _batch_handle<H>(req_body: serde_json::Value, handle: &H) -> Result<serde_json::Value>
where
    H: Handle + Sync,
    H::Request: Debug,
{
    let req: Vec<RPCRequest<Option<H::Request>>> = serde_json::from_value(req_body)?;

    log::debug!("Batch params is: {:?}", &req);

    let r = handle.batch_handle(req).await;

    Ok(serde_json::to_value(r)?)
}

struct HandleHttp<H> {
    handle: Arc<H>,
}

impl<H> Service<Request<Body>> for HandleHttp<H>
where
    H: Handle + Send + Sync + 'static,
    H::Request: Debug,
{
    type Error = Error;

    type Response = Response<Body>;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let handle = self.handle.clone();

        let r = async move {
            let req_body = to_bytes(req.into_body()).await?;

            log::debug!("Request Body: {:?}", req_body);

            let req_body = serde_json::from_slice::<Value>(&req_body)?;

            let body = if req_body.is_object() {
                let r = _handle(req_body, handle.as_ref()).await?;
                Body::from(serde_json::to_string(&r)?)
            } else if req_body.is_array() {
                let r = _batch_handle(req_body, handle.as_ref()).await?;
                Body::from(serde_json::to_string(&r)?)
            } else {
                return Err(anyhow!("Unsupport type"));
            };
            log::debug!("Response Body: {:?}", body);

            let resp = Builder::new()
                .header("Content-Type", "application/json")
                .body(body)?;

            Ok(resp)
        };

        Box::pin(r)
    }
}

struct MakeSvc<H> {
    handle: Arc<H>,
}

impl<T, H> Service<T> for MakeSvc<H>
where
    H: Send + Sync + 'static,
{
    type Response = HandleHttp<H>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let handle = self.handle.clone();
        let fut = async move { Ok(HandleHttp { handle }) };
        Box::pin(fut)
    }
}

pub async fn serve<H>(addr: &SocketAddr, handle: H) -> Result<()>
where
    H: Handle + Send + Sync + 'static,
    H::Request: Debug,
{
    let server = Server::bind(addr).serve(MakeSvc {
        handle: Arc::new(handle),
    });

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
    Ok(())
}
