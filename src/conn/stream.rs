use crate::codec::Frame;

use super::state::State;

#[derive(Debug, Default)]
pub(crate) struct Stream<S: State> {
    state: S,
}

impl<S: State> Stream<S>
where
    S: State,
{
    pub(crate) fn new() -> Self {
        Stream {
            state: S::default(),
        }
    }

    pub(crate) fn parse(&mut self, frame: Frame) -> Result<Option<S::Output>, S::Error> {
        let transition = S::parse_transition(frame)?;

        S::parse_frame(&mut self.state, transition)
    }
}
