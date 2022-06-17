/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::future::Future;
use std::{net::SocketAddr, pin::Pin, task::Poll};

use futures::ready;
use http::{Request as HttpRequest, Response as HttpResponse};
use hyper::body::HttpBody;
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::server::conn::{Connection, Http};
use hyper::Body;
use log::trace;
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite};

use super::Request as JsonRpcRequest;
use super::Response as JsonRpcResponse;

fn wrap_future<F, R, E>(fut: F) -> SrvFut<R, E>
where
    F: Future<Output = Result<R, E>> + Send + 'static,
{
    Box::pin(fut)
}

pin_project! {
   pub struct JsonRpcServer<S> {
        #[pin]
        incoming: AddrIncoming,
        rt_handle: tokio::runtime::Handle,
        service: S
    }
}

impl<S> JsonRpcServer<S> {
    pub fn new(addr: &SocketAddr, handle: tokio::runtime::Handle, service: S) -> Self
    where
        S: tower::Service<HttpRequest<Body>> + Clone,
    {
        let incoming = AddrIncoming::bind(addr).unwrap();
        Self {
            incoming: incoming,
            rt_handle: handle,
            service,
        }
    }

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<AddrStream, std::io::Error>>> {
        let me = self.project();
        me.incoming.poll_accept(cx)
    }
}

type SrvFut<R, E> = Pin<Box<dyn Future<Output = Result<R, E>> + Send + 'static>>;

pin_project! {
    struct OneConnection<IO,S>
    where S: tower::Service<HttpRequest<Body>,Response = HttpResponse<Body>,Error = StdError, Future = SrvFut<HttpResponse<Body>,StdError>>
    {
        #[pin]
        connection: Connection<IO,S>
    }
}

impl<IO, S> Future for OneConnection<IO, S>
where
    S: tower::Service<
            HttpRequest<Body>,
            Response = HttpResponse<Body>,
            Error = StdError,
            Future = SrvFut<HttpResponse<Body>, StdError>,
        > + Unpin,
    IO: AsyncRead + AsyncWrite + Unpin,
{
    type Output = Result<(), hyper::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        self.project().connection.poll_without_shutdown(cx)
    }
}

type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;
impl<S> Future for JsonRpcServer<S>
where
    S: tower::Service<
        HttpRequest<Body>,
        Response = HttpResponse<Body>,
        Error = StdError,
        Future = SrvFut<HttpResponse<Body>, StdError>,
    >,
    S: Clone + Send + 'static + Unpin,
{
    type Output = Result<(), StdError>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        loop {
            let ret = ready!(self.as_mut().poll_next(cx));
            match ret {
                Some(Ok(stream)) => {
                    trace!("Get conn {}", stream.remote_addr());

                    let connection = Http::new()
                        .http1_only(true)
                        .http1_keep_alive(true)
                        .serve_connection(stream, self.service.clone());

                    let one_conn = OneConnection { connection };
                    self.rt_handle.spawn(one_conn);
                }
                Some(Err(e)) => return Poll::Ready(Err(e.into())),
                None => return Poll::Ready(Err("option none".into())),
            }
        }
    }
}

////////////////////////////////////

#[derive(Clone)]
pub struct JsonRpcService<S> {
    service: S,
}

impl<S> JsonRpcService<S> {
    pub fn new(service: S) -> Self
    where
        S: tower::Service<
            JsonRpcRequest,
            Response = JsonRpcResponse,
            Error = StdError,
            Future = SrvFut<JsonRpcResponse, StdError>,
        >,
        S: Clone + Send + 'static,
    {
        Self { service: service }
    }
}

impl<S> tower::Service<HttpRequest<Body>> for JsonRpcService<S>
where
    S: tower::Service<
        JsonRpcRequest,
        Response = JsonRpcResponse,
        Error = StdError,
        Future = SrvFut<JsonRpcResponse, StdError>,
    >,
    S: Clone + Send + 'static,
{
    type Response = HttpResponse<Body>;

    type Error = StdError;

    type Future = SrvFut<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: HttpRequest<Body>) -> Self::Future {
        // serde
        let mut inner_service = self.service.clone();
        wrap_future(async move {
            if let Some(data) = req.data().await {
                if let Err(ref e) = data {
                    trace!("Get body error {}", e);
                }
                let data = data?;

                let request = JsonRpcRequest::from_slice(data.to_vec());

                if let Err(ref e) = request {
                    trace!("Serde error {}", e);
                }
                let request = request?;

                let fut = inner_service.call(request);
                let res = fut.await?;

                let response_string = res.to_string()?;

                return Ok(HttpResponse::builder()
                    .body(response_string.into())
                    .unwrap());
            } else {
                trace!("none");
            }

            trace!("get req {:?}", req);
            Ok(HttpResponse::builder().body(Body::empty()).unwrap())
        })
    }
}