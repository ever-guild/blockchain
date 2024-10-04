// 2022-2024 (c) Copyright Contributors to the GOSH DAO. All rights reserved.
//

use std::collections::HashMap;
use std::sync::Arc;

use tvm_block::Deserializable;
use tvm_block::ShardStateUnsplit;
use tvm_types::Cell;

use crate::block::Block;
use crate::block::WrappedBlock;
use crate::bls::envelope::BLSSignedEnvelope;
use crate::bls::envelope::Envelope;
use crate::bls::GoshBLS;
use crate::database::documents_db::DocumentsDb;
use crate::database::serialize_block::reflect_block_in_db;
// use crate::repository::repository_impl::RepositoryImpl;
// use crate::repository::Repository;

pub mod archive;
pub mod documents_db;
pub mod serialize_block;
pub mod sqlite_helper;

pub fn write_to_db(
    archive: Arc<dyn DocumentsDb>,
    envelope: Envelope<GoshBLS, WrappedBlock>,
    shard_state: Option<Arc<ShardStateUnsplit>>,
    shard_state_cell: Option<Cell>,
    // repository: RepositoryImpl,
) -> anyhow::Result<()> {
    let block = envelope.data().clone();
    let sqlite_clone = archive.clone();
    // let repository_clone = repository.clone();

    std::thread::Builder::new().name("Write block".into()).spawn(move || {
        let shard_state = if let Some(shard_state) = shard_state {
            shard_state
        } else {
            assert!(shard_state_cell.is_some());
            let cell = shard_state_cell.unwrap();
            Arc::new(
                ShardStateUnsplit::construct_from_cell(cell)
                    .expect("Failed to deserialize shard state"),
            )
        };
        tracing::trace!("start reflect_block_in_db in a separate thread");

        // let block_id = block.identifier();
        tracing::trace!(
            "Write to archive: seq_no={:?}, id={:?}",
            block.seq_no(),
            block.identifier()
        );

        let mut transaction_traces = HashMap::new();
        let _changed_acc =
            reflect_block_in_db(sqlite_clone, envelope, shard_state, &mut transaction_traces)
                .map_err(|e| anyhow::format_err!("Failed to archive block data: {e}"))
                .expect("Failed to archive block data");

        // if changed_acc.keys().len() != 0 {
        //     repository_clone
        //         .save_account_diffs(block_id, changed_acc)
        //         .expect("Failed to save account diffs to repository");
        // }
        tracing::trace!("reflect_block_in_db finished");
    })?;

    Ok(())
}
