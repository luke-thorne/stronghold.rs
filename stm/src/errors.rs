// Copyright 2020-2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use thiserror::Error as DeriveError;

#[derive(DeriveError, Debug)]
pub enum TransactionError {
    #[error("Transaction failed {0}")]
    Failed(String),

    #[error("Inner error occured")]
    Inner(String),

    #[error("Transactional state is inconsistent")]
    InconsistentState,

    #[error("Transaction has been aborted")]
    Aborted,
}

impl TransactionError {
    pub fn to_inner<E>(error: E) -> Self
    where
        E: Error + ToString,
    {
        TransactionError::Inner(error.to_string())
    }
}
