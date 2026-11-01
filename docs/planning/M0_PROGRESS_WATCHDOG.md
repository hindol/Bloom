# M0-08 - External progress watchdog

> Status: implemented for M0-08. This does not implement a product scheduler or
> claim that empty-kernel timers progress.

## Decision

Keep the watchdog in the rewrite workspace's integration-test support, outside
`bloom-kernel`. The test runner launches each watched scenario in a child
process. The parent owns a monotonic wall-clock deadline, counts received
progress reports, applies queue and retained-document ceilings, and kills and
reaps the child when a hard bound is exceeded. A hung child cannot block the
observer or rely on its own clock to report a timeout.

The child protocol distinguishes `PROGRESS`, `SUCCEEDED`, `FAILED`, and
`BLOCKED`; process exit without a terminal message is a failure, not success.
The watchdog reports a nonzero exit without a terminal message as
`ExitedWithoutOutcome` with its exit code; `TerminalButNonzeroExit` is reserved
for a terminal report followed by a nonzero exit.
Progress reports include observed queue depth, retained-document count, and
whether an armed timer remains due after the reported update. The observer
counts reports itself rather than trusting a child-supplied step counter or
timestamp.

This test-only process boundary is intentionally not a production worker
protocol. It avoids putting wall-clock reads, process control, or thread
lifecycle into the pure kernel. M0 validates the watchdog against synthetic
success, failure, blocked, resource-limit, due-timer, process-exit, and stall
scenarios. Real kernel transition checks remain pending until M1.

For future repeated open/edit/close journeys, record a baseline retained-
document count, exercise a bounded number of cycles, and require each close to
return to that baseline while queue and document high-water marks remain under
their limits. For I10, every completed update report must have no armed due
timer; the external step/time bound catches a component that never returns an
update report. M0 can test detection with synthetic reports but cannot claim
that a product timer is consumed.

## Verification gate

1. A deliberately stalled child is killed and reported timed out within the
   parent-owned deadline.
2. Success, failure, blocked, process-exit-without-outcome, and malformed
   protocol results are distinct.
3. External report count, queue depth, retained-document count, and due-timer
   state trigger their named watchdog outcomes.
4. The child cannot extend its deadline by reporting its own clock or progress
   timestamp.
5. No production kernel, vault, persistence, or scheduler behavior is added.

The watchdog protocol, child-process observer, and synthetic cases live in
`rewrite/crates/bloom-kernel/tests/progress_watchdog.rs` and its `support`
module.
