//! This module implements a subset of the two phase commit specification presented in the paper
//! ["Consensus on Transaction Commit"](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/tr-2003-96.pdf)
//! by Jim Gray and Leslie Lamport.

use stateright::{Checker, Model, Property};
use std::collections::BTreeSet;
use std::hash::Hash;
use std::ops::Range;

type R = usize; // represented by integers in 0..N-1

#[derive(Clone)]
struct TwoPhaseSys { pub rms: Range<R> }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TwoPhaseState {
    rm_state: Vec<RmState>, // map from each RM
    tm_state: TmState,
    tm_prepared: Vec<bool>, // map from each RM
    msgs: BTreeSet<Message>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Message { Prepared { rm: R }, Commit, Abort }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum RmState { Working, Prepared, Committed, Aborted }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TmState { Init, Committed, Aborted }

#[derive(Clone, Debug)]
enum Action {
    TmRcvPrepared(R),
    TmCommit,
    TmAbort,
    RmPrepare(R),
    RmChooseToAbort(R),
    RmRcvCommitMsg(R),
    RmRcvAbortMsg(R),
}

impl Model for TwoPhaseSys {
    type State = TwoPhaseState;
    type Action = Action;

    fn init_states(&self) -> Vec<Self::State> {
        vec![TwoPhaseState {
            rm_state: self.rms.clone().map(|_| RmState::Working).collect(),
            tm_state: TmState::Init,
            tm_prepared: self.rms.clone().map(|_| false).collect(),
            msgs: Default::default(),
        }]
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        if state.tm_state == TmState::Init && state.tm_prepared.iter().all(|p| *p) {
            actions.push(Action::TmCommit);
        }
        if state.tm_state == TmState::Init {
            actions.push(Action::TmAbort);
        }
        for rm in self.rms.clone() {
            if state.tm_state == TmState::Init
                    && state.msgs.contains(&Message::Prepared { rm: rm.clone() }) {
                actions.push(Action::TmRcvPrepared(rm.clone()));
            }
            if state.rm_state.get(rm) == Some(&RmState::Working) {
                actions.push(Action::RmPrepare(rm.clone()));
            }
            if state.rm_state.get(rm) == Some(&RmState::Working) {
                actions.push(Action::RmChooseToAbort(rm.clone()));
            }
            if state.msgs.contains(&Message::Commit) {
                actions.push(Action::RmRcvCommitMsg(rm.clone()));
            }
            if state.msgs.contains(&Message::Abort) {
                actions.push(Action::RmRcvAbortMsg(rm.clone()));
            }
        }
    }

    fn next_state(&self, last_state: &Self::State, action: Self::Action) -> Option<Self::State> {
        let mut state = last_state.clone();
        match action.clone() {
            Action::TmRcvPrepared(rm) => { state.tm_prepared[rm] = true; }
            Action::TmCommit => {
                state.tm_state = TmState::Committed;
                state.msgs.insert(Message::Commit);
            }
            Action::TmAbort => {
                state.tm_state = TmState::Aborted;
                state.msgs.insert(Message::Abort);
            },
            Action::RmPrepare(rm) => {
                state.rm_state[rm] = RmState::Prepared;
                state.msgs.insert(Message::Prepared { rm });
            },
            Action::RmChooseToAbort(rm) => { state.rm_state[rm] = RmState::Aborted; }
            Action::RmRcvCommitMsg(rm) => { state.rm_state[rm] = RmState::Committed; }
            Action::RmRcvAbortMsg(rm) => { state.rm_state[rm] = RmState::Aborted; }
        }
        Some(state)
    }

    fn properties(&self) -> Vec<Property<Self>> {
        vec![
            Property::<Self>::sometimes("abort agreement", |_, state| {
                state.rm_state.iter().all(|s| s == &RmState::Aborted)
            }),
            Property::<Self>::sometimes("commit agreement", |_, state| {
                state.rm_state.iter().all(|s| s == &RmState::Committed)
            }),
            Property::<Self>::always("consistent", |_, state| {
               !state.rm_state.iter().any(|s1|
                    state.rm_state.iter().any(|s2|
                        s1 == &RmState::Aborted && s2 == &RmState::Committed))
            }),
        ]
    }
}

#[cfg(test)]
#[test]
fn can_model_2pc() {
    // for very small state space (using BFS this time)
    let checker = TwoPhaseSys { rms: 0..3 }.checker().spawn_bfs().join();
    assert_eq!(checker.generated_count(), 288);
    checker.assert_properties();

    // for slightly larger state space (using DFS this time)
    let checker = TwoPhaseSys { rms: 0..5 }.checker().spawn_dfs().join();
    assert_eq!(checker.generated_count(), 8_832);
    checker.assert_properties();
}

fn main() {
    use clap::{App, Arg, SubCommand, value_t};

    env_logger::init_from_env(env_logger::Env::default()
        .default_filter_or("info")); // `RUST_LOG=${LEVEL}` env variable to override

    let mut app = App::new("2pc")
        .about("model check abstract two phase commit")
        .subcommand(SubCommand::with_name("check")
            .about("model check")
            .arg(Arg::with_name("rm_count")
                 .help("number of resource managers")
                 .default_value("7")))
        .subcommand(SubCommand::with_name("explore")
            .about("interactively explore state space")
            .arg(Arg::with_name("rm_count")
                 .help("number of resource managers")
                 .default_value("2"))
            .arg(Arg::with_name("address")
                .help("address Explorer service should listen upon")
                .default_value("localhost:3000")));
    let args = app.clone().get_matches();

    match args.subcommand() {
        ("check", Some(args)) => {
            let rm_count = value_t!(args, "rm_count", usize).expect("rm_count");
            println!("Checking two phase commit with {} resource managers.", rm_count);
            TwoPhaseSys { rms: 0..rm_count }.checker()
                .threads(num_cpus::get()).spawn_dfs()
                .report(&mut std::io::stdout());
        }
        ("explore", Some(args)) => {
            let rm_count = value_t!(args, "rm_count", usize).expect("rm_count");
            let address = value_t!(args, "address", String).expect("address");
            println!("Exploring state space for two phase commit with {} resource managers on {}.", rm_count, address);
            TwoPhaseSys { rms: 0..rm_count }.checker()
                .threads(num_cpus::get())
                .serve(address);
        }
        _ => app.print_help().unwrap(),
    }
}

