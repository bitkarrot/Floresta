#![cfg_attr(any(feature = "no-std", not(test)), no_std)]

pub mod pruned_utreexo;
pub(crate) use floresta_common::prelude;
pub use pruned_utreexo::chain_state::*;
pub use pruned_utreexo::chainparams::*;
pub use pruned_utreexo::chainstore::*;
pub use pruned_utreexo::error::*;
pub use pruned_utreexo::udata::*;
pub use pruned_utreexo::Notification;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Bitcoin,
    Testnet,
    Regtest,
    Signet,
}
impl From<bitcoin::network::constants::Network> for Network {
    fn from(network: bitcoin::network::constants::Network) -> Self {
        match network {
            bitcoin::network::constants::Network::Bitcoin => Network::Bitcoin,
            bitcoin::network::constants::Network::Testnet => Network::Testnet,
            bitcoin::network::constants::Network::Regtest => Network::Regtest,
            bitcoin::network::constants::Network::Signet => Network::Signet,
        }
    }
}
impl From<Network> for bitcoin::network::constants::Network {
    fn from(network: Network) -> Self {
        match network {
            Network::Bitcoin => bitcoin::network::constants::Network::Bitcoin,
            Network::Testnet => bitcoin::network::constants::Network::Testnet,
            Network::Regtest => bitcoin::network::constants::Network::Regtest,
            Network::Signet => bitcoin::network::constants::Network::Signet,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::network::constants::Network as BNetwork;
    #[test]
    fn test_network() {
        assert_eq!(Network::Bitcoin, BNetwork::Bitcoin.into());
        assert_eq!(Network::Testnet, BNetwork::Testnet.into());
        assert_eq!(Network::Regtest, BNetwork::Regtest.into());
        assert_eq!(Network::Signet, BNetwork::Signet.into());
    }
}
