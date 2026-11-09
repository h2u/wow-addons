pub mod backend;
pub mod grouping;
pub mod images;
pub mod inventory;
pub mod provider;

pub mod migration;

#[cfg(all(test, not(windows)))]
#[allow(dead_code)]
mod portable_update;

#[cfg(test)]
#[allow(dead_code)]
mod update_manifest;
