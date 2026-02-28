mod interfaces;
/// Known package detail view module
pub mod known_package_detail;
mod local;
/// Log view module
mod log;
mod package_detail;
/// Packages view module
pub mod packages;
mod search;
mod settings;

pub use interfaces::{InterfacesView, InterfacesViewState};
#[allow(unused_imports)]
pub use known_package_detail::KnownPackageDetailView;
pub use local::LocalView;
pub use log::LogView;
pub use package_detail::PackageDetailView;
pub use packages::PackagesView;
pub use search::{SearchView, SearchViewState};
pub use settings::SettingsView;

pub(crate) use wasm_package_manager::format_size;
