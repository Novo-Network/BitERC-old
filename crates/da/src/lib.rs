#![deny(warnings, unused_crate_dependencies)]

mod da_type;
pub use da_type::*;

mod service;
pub use service::*;

#[cfg(feature = "file")]
mod file_service;
#[cfg(feature = "file")]
pub use file_service::Config as FileConfig;
#[cfg(feature = "file")]
pub use file_service::FileService;

#[cfg(feature = "ipfs")]
mod ipfs_service;
#[cfg(feature = "ipfs")]
pub use ipfs_service::Config as IpfsConfig;
#[cfg(feature = "ipfs")]
pub use ipfs_service::IpfsService;

#[cfg(feature = "celestia")]
mod celestia_service;
#[cfg(feature = "celestia")]
pub use celestia_service::CelestiaService;
#[cfg(feature = "celestia")]
pub use celestia_service::Config as CelestiaConfig;

#[cfg(feature = "greenfield")]
mod greenfield_servic;
#[cfg(feature = "greenfield")]
pub use greenfield_servic::Config as GreenfieldConfig;
#[cfg(feature = "greenfield")]
pub use greenfield_servic::GreenfieldService;
