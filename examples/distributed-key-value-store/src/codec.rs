use std::marker::PhantomData;

use async_std::io;
use futures::{AsyncRead, AsyncWrite};
use libp2p::request_response;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;

pub struct APICodec<P, Request, Response>
where
    P: AsRef<str> + Send + Clone,
    Request: Serialize + DeserializeOwned + Send,
    Response: Serialize + DeserializeOwned + Send,
{
    _protocol: PhantomData<P>,
    _request: PhantomData<Request>,
    _response: PhantomData<Response>,
}

#[async_trait::async_trait()]
impl<P, Request, Response> request_response::Codec for APICodec<P, Request, Response>
where
    P: AsRef<str> + Send + Clone,
    Request: Serialize + DeserializeOwned + Send,
    Response: Serialize + DeserializeOwned + Send,
{
    type Protocol = P;
    type Request = Request;
    type Response = Response;

    async fn read_request<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        unimplemented!("read_request")
    }

    async fn read_response<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        unimplemented!("read_response")
    }

    async fn write_request<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        unimplemented!("write_request")
    }

    async fn write_response<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        unimplemented!("write_response")
    }
}
