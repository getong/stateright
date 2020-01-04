//! This module provides an actor abstraction. See the `model` submodule for a state machine
//! implementation that can check a system of actors. See the `spawn` submodule for a runtime that
//! can run your actor over a real network. See the `register` submodule for an example wrapper.
//!
//! ## Example
//!
//! ```
//! use stateright::*;
//! use stateright::actor::*;
//! use stateright::actor::model::*;
//! use std::iter::FromIterator;
//! use std::sync::Arc;
//!
//! struct ClockActor;
//!
//! impl<Id: Copy> Actor<Id> for ClockActor {
//!     type Msg = u32;
//!     type State = u32;
//!
//!     fn start(&self) -> ActorResult<Id, Self::Msg, Self::State> {
//!         ActorResult::start(0, |_outputs| {})
//!     }
//!
//!     fn advance(&self, state: &Self::State, input: &ActorInput<Id, Self::Msg>) -> Option<ActorResult<Id, Self::Msg, Self::State>> {
//!         let ActorInput::Deliver { src, msg: timestamp } = input;
//!         if timestamp > state {
//!             return ActorResult::advance(state, |state, outputs| {
//!                 *state = *timestamp;
//!                 outputs.send(*src, timestamp + 1);
//!             });
//!         }
//!         return None;
//!     }
//! }
//!
//! let sys = ActorSystem {
//!     actors: vec![ClockActor, ClockActor],
//!     init_network: vec![Envelope { src: 1, dst: 0, msg: 1 }],
//!     lossy_network: LossyNetwork::Yes,
//! };
//! let mut checker = sys.checker(
//!     |sys, snapshot| snapshot.actor_states.iter().all(|s| **s < 3));
//! assert_eq!(
//!     checker.check(100),
//!     CheckResult::Fail {
//!         state: ActorSystemSnapshot {
//!             actor_states: vec![Arc::new(3), Arc::new(2)],
//!             network: Network::from_iter(vec![
//!                 Envelope { src: 1, dst: 0, msg: 1 },
//!                 Envelope { src: 0, dst: 1, msg: 2 },
//!                 Envelope { src: 1, dst: 0, msg: 3 },
//!                 Envelope { src: 0, dst: 1, msg: 4 },
//!             ]),
//!         }
//!     });
//! ```

pub mod model;
pub mod register;
pub mod spawn;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use std::fmt::Debug;
use std::sync::Arc;

/// Inputs to which an actor can respond.
#[derive(Clone, Debug)]
pub enum ActorInput<Id, Msg> {
    Deliver { src: Id, msg: Msg },
}

/// Outputs with which an actor can respond.
#[derive(Clone, Debug)]
pub enum ActorOutput<Id, Msg> {
    Send { dst: Id, msg: Msg },
}

/// We create a wrapper type so we can add convenience methods.
#[derive(Clone, Debug)]
pub struct ActorOutputVec<Id, Msg>(pub Vec<ActorOutput<Id, Msg>>);

impl<Id, Msg> ActorOutputVec<Id, Msg> {
    pub fn send(&mut self, dst: Id, msg: Msg) {
        let ActorOutputVec(outputs) = self;
        outputs.push(ActorOutput::Send { dst, msg })
    }

    pub fn broadcast(&mut self, dsts: &[Id], msg: &Msg)
    where
        Id: Clone,
        Msg: Clone,
    {
        for id in dsts {
            self.send(id.clone(), msg.clone());
        }
    }
}

/// Packages up the state and outputs for an actor step.
#[derive(Debug)]
pub struct ActorResult<Id, Msg, State> {
    pub state: State,
    pub outputs: ActorOutputVec<Id, Msg>,
}

impl<Id, Msg, State> ActorResult<Id, Msg, State> {
    /// Helper for creating a starting result.
    pub fn start<M>(state: State, mutation: M) -> Self
    where M: Fn(&mut ActorOutputVec<Id, Msg>) -> ()
    {
        let mut outputs = ActorOutputVec(Vec::new());
        mutation(&mut outputs);
        ActorResult { state, outputs }
    }

    /// Helper for creating a subsequent result.
    pub fn advance<M>(state: &State, mutation: M) -> Option<Self>
    where
        State: Clone,
        M: Fn(&mut State, &mut ActorOutputVec<Id, Msg>) -> ()
    {
        let mut state = state.clone();
        let mut outputs = ActorOutputVec(Vec::new());
        mutation(&mut state, &mut outputs);
        Some(ActorResult { state, outputs })
    }
}

/// An actor initializes internal state optionally emitting outputs; then it waits for incoming
/// events, responding by updating its internal state and optionally emitting outputs.  At the
/// moment, the only inputs and outputs relate to messages, but other events like timers will
/// likely be added.
pub trait Actor<Id> {
    /// The type of messages sent and received by this actor.
    type Msg;

    /// The type of state maintained by this actor.
    type State;

    /// Indicates the initial state and outputs for the actor.
    fn start(&self) -> ActorResult<Id, Self::Msg, Self::State>;

    /// Indicates the updated state and outputs for the actor when it receives an input.
    fn advance(&self, state: &Self::State, input: &ActorInput<Id, Self::Msg>) -> Option<ActorResult<Id, Self::Msg, Self::State>>;

    /// Indicates how to deserialize messages received by a spawned actor.
    fn deserialize(&self, bytes: &[u8]) -> serde_json::Result<Self::Msg> where Self::Msg: DeserializeOwned {
        serde_json::from_slice(bytes)
    }

    /// Indicates how to serialize messages sent by a spawned actor.
    fn serialize(&self, msg: &Self::Msg) -> serde_json::Result<Vec<u8>> where Self::Msg: Serialize {
        serde_json::to_vec(msg)
    }
}