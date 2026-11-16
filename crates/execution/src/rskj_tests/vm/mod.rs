//! The VM: opcodes, programs, and a transaction's execution (rskj's `co.rsk.vm`
//! and `org.ethereum.vm`).

mod dsl_files_test;
pub(super) mod harness;
mod initcode_cost_calculator_test;
mod transaction_executor_init_code_size_dsl_test;
mod transient_storage_dsl_test;
mod transient_storage_test;
mod vm_dsl_test;
mod vm_execution_test;
mod vm_test;
