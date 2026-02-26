mod client;
pub(crate) use client::Client;

#[cfg(feature = "http-sync")]
pub(crate) mod registry_client;
