use std::marker::PhantomData;

use async_std::io;
use futures::AsyncReadExt;
use futures::AsyncWriteExt;
use futures::{AsyncRead, AsyncWrite};
use libp2p::request_response;
use serde::de::DeserializeOwned;
use serde::ser::Serialize;

#[derive(Clone)]
pub struct APICodec<P, Request, Response>
where
    P: AsRef<str> + Send + Clone,
    Request: Serialize + DeserializeOwned + prost::Message + Default + Send,
    Response: Serialize + DeserializeOwned + Send,
{
    _protocol: PhantomData<P>,
    _request: PhantomData<Request>,
    _response: PhantomData<Response>,
}

impl<P, Request, Response> APICodec<P, Request, Response>
where
    P: AsRef<str> + Send + Clone,
    Request: Serialize + DeserializeOwned + prost::Message + Default + Send,
    Response: Serialize + DeserializeOwned + prost::Message + Default + Send,
{
    pub fn new() -> Self {
        Self {
            _protocol: PhantomData,
            _request: PhantomData,
            _response: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<P, Request, Response> request_response::Codec for APICodec<P, Request, Response>
where
    P: AsRef<str> + Send + Clone,
    Request: Serialize + DeserializeOwned + prost::Message + Default + Send,
    Response: Serialize + DeserializeOwned + prost::Message + Default + Send,
{
    type Protocol = P;
    type Request = Request;
    type Response = Response;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut buf = Vec::new();
        let _ = io.read_to_end(&mut buf).await?;
        prost::Message::decode(buf.as_ref()).map_err(|err| io::Error::other(err))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut buf = Vec::new();
        let _ = io.read_to_end(&mut buf).await?;
        prost::Message::decode(buf.as_ref()).map_err(|err| io::Error::other(err))
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let mut data = Vec::new();
        let _ = req.encode(&mut data).map_err(|err| io::Error::other(err))?;
        let _ = io.write_all(&data).await?;
        io.close().await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let mut data = Vec::new();
        let _ = res.encode(&mut data).map_err(|err| io::Error::other(err))?;
        let _ = io.write_all(&mut data).await?;
        io.close().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::io::Cursor;
    use libp2p::request_response::Codec;

    use super::APICodec;
    use crate::rpc::ApiRequest;
    use crate::rpc::ApiResponse;
    #[tokio::test]
    async fn test_read_request() {
        let mut api_req = ApiRequest::default();
        api_req.key = "foo".as_bytes().to_vec();

        let mut write_buf = Cursor::new(Vec::new());
        let protocol = &"protobuf";
        let mut codec = APICodec::<&'static str, ApiRequest, ApiResponse>::new();
        codec
            .write_request(protocol, &mut write_buf, api_req.clone())
            .await
            .unwrap();

        write_buf.set_position(0);

        let res = codec
            .read_request(&"protobuf", &mut write_buf)
            .await
            .unwrap();
        assert_eq!(res, api_req);

        write_buf.set_position(0);

        let mut api_res = ApiResponse::default();
        api_res.key = "foo".as_bytes().to_vec();
        api_res.value = "bar".as_bytes().to_vec();

        let mut codec = APICodec::<&'static str, ApiRequest, ApiResponse>::new();
        codec
            .write_response(protocol, &mut write_buf, api_res.clone())
            .await
            .unwrap();

        write_buf.set_position(0);
        let res = codec
            .read_response(&"protobuf", &mut write_buf)
            .await
            .unwrap();
        assert_eq!(res, api_res);
    }
}
