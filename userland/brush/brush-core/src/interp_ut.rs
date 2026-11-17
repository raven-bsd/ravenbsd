// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

use super::*;
use crate::{ProfileLoadBehavior, RcLoadBehavior};

/// Every simple command allocates these futures, so keep them small. A rarely
/// taken branch can grow one by a kilobyte if it holds a `Shell` across an
/// `.await`, and clippy's `large_futures` can't see futures whose layout
/// depends on `SE`. The limits leave room for compiler updates; if one is
/// exceeded, find out why before raising it.
#[tokio::test]
async fn futures_for_a_simple_command_stay_small() -> anyhow::Result<()> {
    let mut shell = Shell::builder()
        .profile(ProfileLoadBehavior::Skip)
        .rc(RcLoadBehavior::Skip)
        .build()
        .await?;
    let params = shell.default_exec_params();

    let program = shell.parse_string("true")?;
    let list = &program.complete_commands[0];
    let and_or = &list.0[0].0;
    let pipeline = &and_or.first;
    let command = &pipeline.seq[0];
    let ast::Command::Simple(simple) = command else {
        anyhow::bail!("expected a simple command");
    };

    let mut sizes = vec![];
    let context = PipelineExecutionContext {
        shell: commands::ShellForCommand::ParentShell(&mut shell),
        process_group_id: None,
    };
    let future = simple.execute_in_pipeline(context, params.clone());
    sizes.push((
        "SimpleCommand::execute_in_pipeline",
        size_of_val(&*future),
        3072,
    ));
    drop(future);

    let context = PipelineExecutionContext {
        shell: commands::ShellForCommand::ParentShell(&mut shell),
        process_group_id: None,
    };
    let future = command.execute_in_pipeline(context, params.clone());
    sizes.push((
        "Command::execute_in_pipeline",
        size_of_val(&*future),
        // Rustc 1.97 lays this future out at 784 bytes. The previous 768 limit was tied
        // to an older compiler; the guard still catches the kilobyte-sized regressions it
        // exists for.
        800,
    ));
    drop(future);

    let future = pipeline.execute(&mut shell, &params);
    sizes.push(("Pipeline::execute", size_of_val(&*future), 768));
    drop(future);

    let future = and_or.execute(&mut shell, &params);
    sizes.push(("AndOrList::execute", size_of_val(&*future), 256));
    drop(future);

    let future = list.execute(&mut shell, &params);
    sizes.push(("CompoundList::execute", size_of_val(&*future), 128));
    drop(future);

    for (name, size, limit) in sizes {
        assert!(
            size <= limit,
            "{name}'s future takes {size} bytes; the limit is {limit}"
        );
    }

    Ok(())
}
