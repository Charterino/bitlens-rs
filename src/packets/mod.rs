use supercow::Supercow;

pub mod addr;
pub mod addrv2;
pub mod block;
pub mod blockheader;
pub mod buffer;
pub mod deepclone;
pub mod getaddr;
pub mod getdata;
pub mod getheaders;
pub mod headers;
pub mod inv;
pub mod invvector;
pub mod magic;
pub mod netaddr;
pub mod network_id;
pub mod packet;
pub mod packetheader;
pub mod packetpayload;
pub mod ping;
pub mod pong;
pub mod sendaddrv2;
pub mod sendheaders;
pub mod tx;
pub mod varint;
pub mod varstr;
pub mod verack;
pub mod version;

#[derive(Debug)]
pub struct SupercowVec<'a, T> {
    pub inner: Supercow<'a, Vec<Supercow<'a, T>>, [Supercow<'a, T>]>,
}

impl<T> Default for SupercowVec<'_, T> {
    fn default() -> Self {
        Self {
            inner: Supercow::owned(vec![]),
        }
    }
}

impl<T: Clone> Clone for SupercowVec<'_, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> SupercowVec<'_, T> {
    pub fn from_owned(data: Vec<T>) -> Self {
        let mapped_to_supercows = data.into_iter().map(|item| Supercow::owned(item)).collect();
        Self {
            inner: Supercow::owned(mapped_to_supercows),
        }
    }
}
