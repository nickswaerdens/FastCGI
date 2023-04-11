use crate::codec::Frame;

use super::state::{ParseError, State};

#[derive(Debug)]
pub(crate) struct Stream<S: State> {
    state: S,
}

impl<S: State> Stream<S>
where
    S: State,
{
    pub(crate) fn parse(&mut self, frame: Frame) -> Result<Option<S::Output>, ParseError> {
        let transition = S::parse_transition(frame)?;

        S::parse_frame(&mut self.state, transition)
    }
}

impl<S> Default for Stream<S>
where
    S: State + Default,
{
    fn default() -> Self {
        Self {
            state: S::default(),
        }
    }
}
