use vc_model::{Duration, LoadStage};

use super::{StageCommand, StageController, StageInput};

fn stage(millis: u64, users: u32) -> LoadStage {
    LoadStage { duration: Duration::new(std::time::Duration::from_millis(millis)), users }
}

/// The users after each tick, and every command, over `ticks` ticks.
fn run(stages: Vec<LoadStage>, ticks: usize) -> (Vec<u32>, Vec<StageCommand>) {
    let mut controller = StageController::new(stages);
    let mut users = Vec::new();
    let mut commands = Vec::new();
    for _ in 0..ticks {
        let (next, issued) = controller.transition(StageInput::Tick);
        controller = next;
        users.push(controller.users());
        commands.extend(issued);
    }
    (users, commands)
}

#[test]
fn users_ramp_linearly_from_zero_then_from_the_previous_stage() {
    let (users, _) = run(vec![stage(4000, 8), stage(2000, 8), stage(4000, 0)], 11);
    assert_eq!(users, [0, 2, 4, 6, 8, 8, 8, 6, 4, 2, 0]);
}

#[test]
fn each_tick_spawns_or_retires_the_difference_and_the_end_retires_everyone() {
    let (_, commands) = run(vec![stage(2000, 4), stage(1000, 4)], 5);
    assert_eq!(
        commands,
        [StageCommand::Spawn(2), StageCommand::Spawn(2), StageCommand::Retire(4), StageCommand::Finish]
    );
}

#[test]
fn a_ramp_rounds_to_the_nearest_user() {
    let (users, _) = run(vec![stage(3000, 2)], 3);
    assert_eq!(users, [0, 1, 1]);
}

#[test]
fn a_zero_length_stage_only_sets_where_the_next_ramp_starts() {
    let (users, _) = run(vec![stage(0, 10), stage(2000, 0)], 3);
    assert_eq!(users, [10, 5, 0]);
}

#[test]
fn a_part_second_stage_ends_at_the_next_tick() {
    let (users, commands) = run(vec![stage(1500, 3)], 3);
    assert_eq!(users, [0, 2, 0]);
    assert_eq!(commands.last(), Some(&StageCommand::Finish));
}

#[test]
fn abort_retires_every_user_and_later_input_does_nothing() {
    let (controller, _) = StageController::new(vec![stage(1000, 5), stage(10_000, 5)]).transition(StageInput::Tick);
    let (controller, _) = controller.transition(StageInput::Tick);
    assert_eq!(controller.users(), 5);
    let (controller, commands) = controller.transition(StageInput::Abort);
    assert_eq!(commands, [StageCommand::Retire(5), StageCommand::Finish]);
    assert!(controller.finished());
    let (controller, commands) = controller.transition(StageInput::Tick);
    assert_eq!((controller.users(), commands), (0, Vec::new()));
}

#[test]
fn no_stages_finish_at_once() {
    let (users, commands) = run(Vec::new(), 1);
    assert_eq!((users, commands), (vec![0], vec![StageCommand::Finish]));
}
