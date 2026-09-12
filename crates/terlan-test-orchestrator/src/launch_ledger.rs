//! Live direct-child observations; not input-bound reusable test receipts.

use super::*;
use report_file::ReportFile;

#[derive(PartialEq, Eq)]
enum LedgerState {
    Running,
    Failed,
    Sealed,
}

/// Serializes phase attempts, actual child identities, and terminal observations.
pub(super) struct LaunchLedger {
    report: ReportFile,
    started: Instant,
    timeout: Duration,
    threads: usize,
    results: Vec<PhaseResult>,
    state: LedgerState,
    inputs: validation_inputs::ValidationInputs,
}

impl LaunchLedger {
    /// Owns and initializes the live snapshot before the first producer starts.
    pub(super) fn new(path: &Path, threads: usize, timeout: Duration) -> Result<Self, String> {
        let mut ledger = Self {
            report: ReportFile::open(path)?,
            started: Instant::now(),
            timeout,
            threads,
            results: Vec::new(),
            state: LedgerState::Running,
            inputs: validation_inputs::ValidationInputs::default(),
        };
        ledger.save("running")?;
        Ok(ledger)
    }

    /// Requires an observed spawn and persists both running and terminal states.
    pub(super) fn execute<T>(
        &mut self,
        name: &'static str,
        tier: ValidationTier,
        executor: &'static str,
        operation: impl FnOnce(&mut dyn FnMut(u32) -> Result<(), String>) -> Result<T, PhaseFailure>,
    ) -> Result<T, PhaseFailure> {
        self.execute_observed(name, tier, executor, operation, |_| None)
    }

    /// Persists verified completion or explicitly partial failure observations with the phase.
    pub(super) fn execute_test(
        &mut self,
        name: &'static str,
        tier: ValidationTier,
        executor: &'static str,
        operation: impl FnOnce(
            &mut dyn FnMut(u32) -> Result<(), String>,
            &mut Option<test_execution::TestEvidence>,
        ) -> Result<test_execution::TestEvidence, PhaseFailure>,
    ) -> Result<test_execution::TestEvidence, PhaseFailure> {
        let partial = std::cell::RefCell::new(None);
        self.execute_observed(
            name,
            tier,
            executor,
            |launched| operation(launched, &mut partial.borrow_mut()),
            |result| {
                result
                    .as_ref()
                    .ok()
                    .map(test_execution::TestEvidence::json)
                    .or_else(|| {
                        partial
                            .borrow()
                            .as_ref()
                            .map(test_execution::TestEvidence::json)
                    })
            },
        )
    }

    fn execute_observed<T>(
        &mut self,
        name: &'static str,
        tier: ValidationTier,
        executor: &'static str,
        operation: impl FnOnce(&mut dyn FnMut(u32) -> Result<(), String>) -> Result<T, PhaseFailure>,
        observation: impl FnOnce(&Result<T, PhaseFailure>) -> Option<serde_json::Value>,
    ) -> Result<T, PhaseFailure> {
        if self.state != LedgerState::Running {
            return Err(failure("suite is failed or already sealed"));
        }
        let duplicate = self
            .results
            .iter()
            .any(|result| result.name == name || result.outcome != "passed");
        let over_budget = executor.starts_with("cargo")
            && self
                .results
                .iter()
                .filter(|result| result.child_pid.is_some() && result.executor.starts_with("cargo"))
                .count()
                >= MAX_CARGO_PHASES;
        if duplicate || over_budget {
            self.state = LedgerState::Failed;
            self.save("fail").map_err(failure)?;
            return Err(failure(format!(
                "duplicate phase owner or Cargo launch budget exceeded: {name}"
            )));
        }
        let index = self.results.len();
        let started = Instant::now();
        self.results.push(PhaseResult {
            name,
            tier,
            executor,
            wall_time_ms: 0,
            outcome: "starting",
            child_pid: None,
            test_execution: None,
        });
        self.save("running").map_err(failure)?;
        let mut observation_failed = false;
        let mut outcome = operation(&mut |pid| {
            if pid == 0 || self.results[index].child_pid.is_some() {
                observation_failed = true;
                return Err("invalid or repeated child observation".into());
            }
            self.results[index].child_pid = Some(pid);
            self.results[index].outcome = "running";
            self.results[index].wall_time_ms = started.elapsed().as_millis();
            self.save("running")
                .inspect_err(|_| observation_failed = true)
        });
        if observation_failed || (outcome.is_ok() && self.results[index].child_pid.is_none()) {
            outcome = Err(failure("phase has missing or invalid launch observations"));
        }
        self.results[index].test_execution = observation(&outcome);
        if outcome.is_ok()
            && matches!(
                executor,
                "cargo" | "cargo-native-harnesses" | "direct-terlan-harness"
            )
            && self.results[index].test_execution.is_none()
        {
            outcome = Err(failure("test phase has no verified execution summary"));
        }
        self.results[index].wall_time_ms = started.elapsed().as_millis();
        self.results[index].outcome = outcome
            .as_ref()
            .map_or_else(|error| error.outcome, |_| "passed");
        if outcome.is_err() {
            self.state = LedgerState::Failed;
        }
        self.save(if outcome.is_ok() { "running" } else { "fail" })
            .map_err(failure)?;
        outcome
    }

    /// Seals success only after every recorded producer has succeeded.
    #[cfg(test)]
    pub(super) fn finish(&mut self) -> Result<(), String> {
        self.finish_unless_cancelled(&std::sync::atomic::AtomicBool::new(false))
    }

    /// Cancellation observed before closeout cannot produce a passing snapshot.
    pub(super) fn finish_unless_cancelled(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        if cancelled.load(std::sync::atomic::Ordering::Acquire)
            || (self.inputs.source.before.is_some() && !self.inputs.source.verified())
            || (self.inputs.executables.is_bound() && !self.inputs.executables.verified())
            || (self.inputs.configuration.is_bound() && !self.inputs.configuration.verified())
            || (self.inputs.cargo_tools.is_bound() && !self.inputs.cargo_tools.verified())
            || (self.inputs.toolchain.is_bound() && !self.inputs.toolchain.verified())
            || (self.inputs.compiler.is_bound() && !self.inputs.compiler.verified())
            || self.inputs.metadata.as_ref().is_some_and(|metadata| {
                let observation = metadata.json();
                observation["resolver_cache"]["verified"] != true
                    || observation["package_sources"]["verified"] != true
            })
            || (self.inputs.metadata.is_some() && self.inputs.test_selections.is_none())
            || self
                .inputs
                .test_selections
                .as_ref()
                .is_some_and(|selections| selections.json()["verified"] != true)
        {
            self.state = LedgerState::Failed;
            self.save("fail")?;
            return Err("suite was cancelled or inputs were not verified before closeout".into());
        }
        if self.state != LedgerState::Running
            || self.results.is_empty()
            || self.results.iter().any(|result| result.outcome != "passed")
        {
            return Err("cannot pass an empty or unsuccessful suite".into());
        }
        self.save("pass")?;
        self.state = LedgerState::Sealed;
        Ok(())
    }

    /// Records terminal failure even when declaration rejection precedes an observed phase.
    pub(super) fn reject_input_verification(&mut self) -> Result<(), PhaseFailure> {
        if self.state != LedgerState::Failed {
            self.state = LedgerState::Failed;
            self.save("fail").map_err(failure)?;
        }
        Ok(())
    }

    /// Seals a current-input observation without claiming that any tests ran here.
    pub(super) fn finish_input_verification(
        &mut self,
        previous: &serde_json::Value,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), PhaseFailure> {
        self.finish_coverage(previous, cancelled, false)
    }

    /// Checks historical input identity before an owned Make process may reuse test outcomes.
    pub(super) fn compare_admission(
        &self,
        previous: &serde_json::Value,
    ) -> Result<(), PhaseFailure> {
        crate::suite_inputs::compare_admission(&self.inputs.snapshot(), previous)
    }

    /// Seals owned gate execution separately from read-only input verification.
    pub(super) fn finish_owned_coverage(
        &mut self,
        previous: &serde_json::Value,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), PhaseFailure> {
        self.finish_coverage(previous, cancelled, true)
    }

    fn finish_coverage(
        &mut self,
        previous: &serde_json::Value,
        cancelled: &std::sync::atomic::AtomicBool,
        owned_make: bool,
    ) -> Result<(), PhaseFailure> {
        let result = (|| {
            let make_phases = self
                .results
                .iter()
                .filter(|phase| phase.executor == "make-covered-gates")
                .collect::<Vec<_>>();
            if make_phases.len() != usize::from(owned_make)
                || make_phases.iter().any(|phase| {
                    phase
                        .test_execution
                        .as_ref()
                        .is_none_or(|value| value["decision"] != "pass")
                })
            {
                return Err(failure(
                    "Make coverage lacks a completed live request owner",
                ));
            }
            if cancelled.load(std::sync::atomic::Ordering::Acquire)
                || self.state != LedgerState::Running
                || !self.inputs.fully_verified()
                || self.results.is_empty()
                || self.results.iter().any(|phase| {
                    phase.outcome != "passed"
                        || phase.executor.starts_with("cargo")
                        || phase.executor.starts_with("direct-")
                })
            {
                return Err(failure(
                    "input verification is incomplete or executed a producer",
                ));
            }
            crate::suite_inputs::compare(&self.inputs.snapshot(), previous)?;
            if previous["test_threads"] != self.threads
                || previous["phase_timeout_seconds"] != self.timeout.as_secs()
            {
                return Err(failure(
                    "test execution policy differs from completed suite",
                ));
            }
            Ok(())
        })();
        self.state = if result.is_ok() {
            LedgerState::Sealed
        } else {
            LedgerState::Failed
        };
        // Deliberately not `pass`: this observation contains no test execution.
        self.save(if result.is_ok() && owned_make {
            "gates-covered"
        } else if result.is_ok() {
            "inputs-verified"
        } else {
            "fail"
        })
        .map_err(failure)?;
        result
    }

    /// Requires the current source snapshot to match the completed hosted source exactly.
    pub(super) fn compare_hosted_source(
        &self,
        hosted: &serde_json::Value,
    ) -> Result<(), PhaseFailure> {
        let current = self.inputs.source.json();
        if current["before"].is_null()
            || hosted["verified"] != true
            || hosted["before"] != hosted["after"]
            || current["before"] != hosted["before"]
        {
            return Err(failure(
                "current source differs from the hosted test source",
            ));
        }
        Ok(())
    }

    /// Seals source-only reuse without claiming remote/local binary or environment equivalence.
    pub(super) fn finish_hosted_source(
        &mut self,
        hosted: &serde_json::Value,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), PhaseFailure> {
        let owners = self
            .results
            .iter()
            .filter(|phase| phase.executor == "hosted-source-covered-gates")
            .collect::<Vec<_>>();
        if cancelled.load(std::sync::atomic::Ordering::Acquire)
            || self.state != LedgerState::Running
            || !self.inputs.source.verified()
            || !self.inputs.executables.verified()
            || self.inputs.environment.is_null()
            || owners.len() != 1
            || owners[0].test_execution.as_ref().is_none_or(|evidence| {
                evidence["decision"] != "pass"
                    || evidence["scope"] != "hosted-canonical-source-test-coverage-v1"
                    || evidence["local_artifact_test_equivalence"] != false
            })
            || self.results.iter().any(|phase| {
                phase.outcome != "passed"
                    || phase.executor.starts_with("cargo")
                    || phase.executor.starts_with("direct-")
            })
        {
            self.reject_input_verification()?;
            return Err(failure("hosted source gate execution did not close"));
        }
        self.compare_hosted_source(hosted)?;
        self.state = LedgerState::Sealed;
        self.save("hosted-source-gates-covered").map_err(failure)
    }

    /// Persists admission evidence once, before the source-bound producers run.
    pub(super) fn bind_source(
        &mut self,
        source: source_inventory::SourceSnapshot,
    ) -> Result<(), String> {
        if self.state != LedgerState::Running
            || self.inputs.source.before.is_some()
            || self.results.iter().any(|result| {
                result.executor != "git-source-inventory" || result.outcome != "passed"
            })
        {
            self.state = LedgerState::Failed;
            self.save("fail")?;
            return Err("source admission is already bound or suite is not running".into());
        }
        self.inputs.source.before = Some(source);
        self.save("running")
    }

    /// Retains both observations and fails closed when source inputs changed.
    pub(super) fn verify_source(
        &mut self,
        source: source_inventory::SourceSnapshot,
    ) -> Result<(), String> {
        if self.state != LedgerState::Running || self.inputs.source.after.is_some() {
            self.state = LedgerState::Failed;
            self.save("fail")?;
            return Err("source closeout is already recorded or suite is not running".into());
        }
        self.inputs.source.after = Some(source);
        if !self.inputs.source.verified() {
            self.state = LedgerState::Failed;
            self.save("fail")?;
            return Err("admission and closeout source observations differ".into());
        }
        self.save("running")
    }

    /// Admits a fresh shared metadata generation before suite build execution.
    pub(super) fn admit_metadata(
        &mut self,
        environment: &execution_environment::ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if self.results.iter().any(|phase| {
            phase.executor.starts_with("cargo") || phase.executor.starts_with("direct-")
        }) {
            return self.reject_inputs("metadata admission must precede builds and test execution");
        }
        self.observe_inputs(|inputs| {
            if inputs.metadata.is_some() || inputs.source.before.is_none() {
                return Err(failure(
                    "metadata requires source admission and cannot repeat",
                ));
            }
            inputs.metadata = Some(cargo_metadata_owner::Handoff::admit(
                inputs,
                environment,
                control,
            )?);
            Ok(())
        })
    }

    /// Exposes the admitted package generation without another metadata query.
    pub(super) fn metadata(&self) -> Result<&cargo_metadata_owner::Handoff, PhaseFailure> {
        self.inputs
            .metadata
            .as_ref()
            .ok_or_else(|| failure("missing suite metadata admission"))
    }

    /// Retains exact already-admitted names before any main-harness test execution.
    pub(super) fn admit_test_selections(
        &mut self,
        phases: &[TestPhase],
        selections: &test_inventory::TestPlan,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if self
            .results
            .iter()
            .any(|result| result.executor == "direct-terlan-harness")
        {
            return self.reject_inputs("test selection admission must precede execution");
        }
        let path = self.report.selections_path();
        let run_id = self.report.run_id().to_owned();
        self.observe_inputs(|inputs| {
            if inputs.test_selections.is_some() {
                return Err(failure("test selection admission cannot repeat"));
            }
            let executables = inputs.executables.json();
            let harness = executables["before"]
                .as_array()
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row["role"] == "terlan-library-harness")
                })
                .ok_or_else(|| failure("test selections require the admitted main harness"))?;
            inputs.test_selections = Some(test_selections::TestSelections::create(
                &path, &run_id, phases, selections, harness, control,
            )?);
            Ok(())
        })
    }

    /// Reconciles exact selections against completed phase records and retained bytes.
    pub(super) fn verify_test_selections(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        let results = self.results.clone();
        self.observe_inputs(|inputs| {
            inputs
                .test_selections
                .as_mut()
                .ok_or_else(|| failure("missing test selections"))?
                .verify(&results, control)
        })
    }

    /// Shares selected Rustdoc tools and the already-admitted workspace target inventory.
    pub(super) fn rustdoc_inputs(
        &self,
        control: ProcessControl<'_>,
    ) -> Result<crate::rustdoc_owner::Inputs, PhaseFailure> {
        Ok(crate::rustdoc_owner::Inputs {
            rustdoc: self.inputs.cargo_tools.rustdoc(control)?,
            targets: self.metadata()?.doctest_targets(),
            settings: self.inputs.configuration.cargo_settings().clone(),
        })
    }

    /// Closes the resolver-cache observation alongside source and tools.
    pub(super) fn verify_metadata(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_inputs(|inputs| {
            inputs
                .metadata
                .as_mut()
                .ok_or_else(|| failure("missing suite metadata admission"))?
                .verify(control)
        })
    }

    /// Records the frozen environments before any producer or executable admission.
    pub(super) fn bind_environment(
        &mut self,
        environment: &execution_environment::ExecutionEnvironment,
        phases: &[TestPhase],
    ) -> Result<(), String> {
        if self.state != LedgerState::Running
            || !self.results.is_empty()
            || !self.inputs.environment.is_null()
            || self.inputs.executables.is_bound()
        {
            self.state = LedgerState::Failed;
            self.save("fail")?;
            return Err("environment admission must precede producers and cannot repeat".into());
        }
        self.inputs.environment = environment.json(phases);
        self.save("running")
    }

    /// Records selected entry points and prebuilt runtimes before any launch.
    pub(super) fn admit_executables(
        &mut self,
        environment: &execution_environment::ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if !self.results.is_empty() {
            return self.reject_inputs("executable admission must precede producers");
        }
        if self.inputs.environment.is_null()
            || self.inputs.environment["inherited_identity_sha256"]
                != environment.json(&[])["inherited_identity_sha256"]
        {
            return self.reject_inputs("executable admission requires the bound environment");
        }
        self.observe_executables(|binding| {
            if binding.is_bound() {
                return Err(failure("executable admission is already bound"));
            }
            *binding = executable_binding::ExecutableBinding::capture(
                &executable_paths(environment)?,
                control,
            )?;
            Ok(())
        })
    }

    /// Admits ambient tool-selection files before starting any producer.
    pub(super) fn admit_configuration(
        &mut self,
        environment: &execution_environment::ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if !self.results.is_empty()
            || self.inputs.environment.is_null()
            || self.inputs.environment["inherited_identity_sha256"]
                != environment.json(&[])["inherited_identity_sha256"]
        {
            return self.reject_inputs(
                "configuration admission requires the bound environment before producers",
            );
        }
        self.observe_configuration(|binding| {
            if binding.is_bound() {
                return Err(failure("tool configuration is already bound"));
            }
            *binding = tool_configuration::ToolConfiguration::capture(environment, control)?;
            Ok(())
        })
    }

    /// Rejects configuration edits, creation, removal, and retargeting at closeout.
    pub(super) fn verify_configuration(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_configuration(|binding| binding.verify(control))
    }

    fn observe_configuration(
        &mut self,
        operation: impl FnOnce(&mut tool_configuration::ToolConfiguration) -> Result<(), PhaseFailure>,
    ) -> Result<(), PhaseFailure> {
        self.observe_inputs(|inputs| operation(&mut inputs.configuration))
    }

    /// Binds configured compiler/wrapper paths before admitting a producer.
    pub(super) fn admit_cargo_tools(
        &mut self,
        environment: &execution_environment::ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if self.results.iter().any(|phase| {
            phase.outcome != "passed"
                || !matches!(
                    phase.executor,
                    "git-source-inventory" | "rustup-tool-resolution" | "rust-tool-version"
                )
        }) || !self.inputs.configuration.is_bound()
            || self.inputs.environment["inherited_identity_sha256"]
                != environment.json(&[])["inherited_identity_sha256"]
        {
            return self
                .reject_inputs("Cargo tool selection requires admitted settings before producers");
        }
        self.observe_inputs(|inputs| {
            if inputs.cargo_tools.is_bound() {
                return Err(failure("Cargo tools are already bound"));
            }
            let proxy = if inputs.executables.same_identity("cargo", "rustup") {
                let root = inputs.toolchain.root().ok_or_else(|| {
                    failure("Rustup Cargo tool selection requires the admitted toolchain")
                })?;
                let name = inputs.toolchain.proxy_name().ok_or_else(|| {
                    failure("Rustup Cargo tool selection requires its observed active name")
                })?;
                Some((root, name))
            } else {
                None
            };
            inputs.cargo_tools = configured_cargo_tools::ConfiguredCargoTools::capture_selected(
                inputs.configuration.cargo_settings(),
                environment,
                proxy,
                control,
            )?;
            if inputs.toolchain.is_bound() {
                inputs.cargo_tools.prepare_compiler_environment(
                    inputs.configuration.cargo_settings(),
                    environment,
                    &inputs.toolchain,
                    control,
                )?;
            }
            Ok(())
        })
    }

    /// Requires unchanged configured tool paths and bytes at suite closeout.
    pub(super) fn verify_cargo_tools(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_inputs(|inputs| inputs.cargo_tools.verify(control))
    }

    /// Admits the actual compiler only after its selected tools and environment are bound.
    pub(super) fn admit_compiler(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if self.inputs.compiler.is_bound()
            || !self.inputs.toolchain.is_bound()
            || self
                .results
                .iter()
                .any(|phase| phase.executor.starts_with("cargo"))
        {
            return self.reject_inputs(
                "compiler admission requires bound tools before any Cargo producer",
            );
        }
        let invocations = self.inputs.cargo_tools.compiler_invocations(control);
        let known = self.inputs.toolchain.admitted_tree().cloned();
        let compiler = invocations.and_then(|invocations| {
            crate::selected_compiler::SelectedCompiler::admit(
                invocations,
                known.as_ref(),
                self,
                control,
            )
        });
        match compiler {
            Ok(compiler) => self.observe_inputs(|inputs| {
                inputs.compiler = compiler;
                Ok(())
            }),
            Err(error) => {
                self.state = LedgerState::Failed;
                self.save("fail").map_err(failure)?;
                Err(error)
            }
        }
    }

    /// Shares the verified installed tree instead of hashing it a second time.
    pub(super) fn verify_compiler(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_inputs(|inputs| {
            inputs
                .compiler
                .verify(inputs.toolchain.verified_tree(), control)
        })
    }

    /// Records installed-toolchain admission after successful Rustup resolution.
    pub(super) fn bind_toolchain(
        &mut self,
        toolchain: rust_toolchain::RustToolchain,
    ) -> Result<(), String> {
        if self.results.last().is_none_or(|phase| {
            phase.executor != "rustup-tool-resolution" || phase.outcome != "passed"
        }) || self
            .results
            .iter()
            .any(|phase| phase.executor.starts_with("cargo"))
        {
            return self
                .reject_inputs("Rust toolchain admission must follow its probe and precede builds")
                .map_err(|error| error.detail);
        }
        self.observe_inputs(|inputs| {
            if inputs.toolchain.is_bound() || !toolchain.is_bound() {
                return Err(failure("Rust toolchain admission is absent or repeated"));
            }
            inputs.toolchain = toolchain;
            Ok(())
        })
        .map_err(|error| error.detail)
    }

    /// Persists admission failures that happen between observed resolver probes.
    pub(super) fn reject_toolchain(&mut self) -> Result<(), String> {
        self.state = LedgerState::Failed;
        self.save("fail")
    }

    /// Requires the installation and its native entry points to match at closeout.
    pub(super) fn verify_toolchain(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_inputs(|inputs| inputs.toolchain.verify(control))
    }

    /// Extends admission only with the harness produced by this Cargo phase.
    #[cfg(test)]
    pub(super) fn bind_harness(
        &mut self,
        path: &Path,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_executables(|binding| binding.bind_harness(path, control))
    }

    /// Binds CLI/runtime build outputs before entering any direct or Cargo test phase.
    pub(super) fn bind_test_programs(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_executables(|binding| binding.bind_test_programs(control))
    }

    /// Production admission binds the declared harness contract alongside executable bytes.
    pub(super) fn bind_declared_harness(
        &mut self,
        declaration: cargo_harness_admission::DeclaredHarness,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_executables(|binding| binding.bind_declared_harness(declaration, control))
    }

    /// Borrows immutable admission evidence for pre-launch harness checks.
    pub(super) fn executables(&self) -> &executable_binding::ExecutableBinding {
        &self.inputs.executables
    }

    /// Requires all selected executable files to still match before sealing.
    pub(super) fn verify_executables(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.observe_executables(|binding| binding.verify(control))
    }

    fn observe_executables(
        &mut self,
        operation: impl FnOnce(&mut executable_binding::ExecutableBinding) -> Result<(), PhaseFailure>,
    ) -> Result<(), PhaseFailure> {
        self.observe_inputs(|inputs| operation(&mut inputs.executables))
    }

    fn observe_inputs(
        &mut self,
        operation: impl FnOnce(&mut validation_inputs::ValidationInputs) -> Result<(), PhaseFailure>,
    ) -> Result<(), PhaseFailure> {
        if self.state != LedgerState::Running {
            return Err(failure("suite is not running"));
        }
        let result = operation(&mut self.inputs);
        if result.is_err() {
            self.state = LedgerState::Failed;
        }
        self.save(if result.is_ok() { "running" } else { "fail" })
            .map_err(failure)?;
        result
    }

    fn reject_inputs(&mut self, message: &str) -> Result<(), PhaseFailure> {
        self.state = LedgerState::Failed;
        self.save("fail").map_err(failure)?;
        Err(failure(message))
    }

    fn save(&mut self, decision: &str) -> Result<(), String> {
        let result = write_report(
            &mut self.report,
            decision,
            self.threads,
            self.timeout,
            self.started.elapsed(),
            &self.results,
            &self.inputs,
        );
        if result.is_err() {
            self.state = LedgerState::Failed;
        }
        result
    }
}

fn failure(message: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "inventory-failed",
        detail: message.to_string(),
    }
}

#[cfg(test)]
#[path = "launch_ledger_test.rs"]
mod tests;
