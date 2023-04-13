use core::fmt;

use super::state;

pub(crate) trait Endpoint {
    type State: state::State + fmt::Debug;
}

#[derive(Debug)]
pub(crate) enum Client {}

#[derive(Debug)]
pub(crate) enum Server {}

impl Endpoint for Client {
    type State = state::client::State;
}

impl Endpoint for Server {
    type State = state::server::State;
}
