//! Navigation, Smart Breadcrumb, and Favorites subsystem.

pub mod breadcrumb;
pub mod favorites;

pub use breadcrumb::{BreadcrumbSegment, SmartBreadcrumb};
pub use favorites::{FavoriteItem, FavoriteList};
