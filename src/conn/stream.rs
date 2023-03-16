use std::marker::PhantomData;

use crate::codec::Frame;

use super::parser::{Parser, ParserError};

#[derive(Debug)]
pub(crate) struct Stream<P: Parser> {
    state: P::State,
    _parser: PhantomData<P>,
}

impl<P: Parser> Stream<P> {
    pub fn parse_frame(&mut self, frame: Frame) -> Result<Option<P::Output>, ParserError> {
        let transition = P::parse_transition(frame)?;

        P::parse_frame(&mut self.state, transition)
    }
}

impl<P> Default for Stream<P>
where
    P: Parser,
{
    fn default() -> Self {
        Self {
            state: P::State::default(),
            _parser: PhantomData,
        }
    }
}
