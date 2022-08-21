use aws_sdk_rds::{
    error::{CreateDBInstanceError, DescribeDBInstancesError},
    types::SdkError,
};
use thiserror::Error;
use tonic::Status;
use tracing::error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to create role")]
    CreateRole(String),

    #[error("failed to update role")]
    UpdateRole(String),

    #[error("failed to create DB")]
    CreateDB(String),

    #[error("unexpected sqlx error")]
    UnexpectedSqlx(#[from] sqlx::Error),

    #[error("unexpected mongodb error")]
    UnexpectedMongodb(#[from] mongodb::error::Error),

    #[error("failed to create RDS instance")]
    CreateRDSInstance(#[from] SdkError<CreateDBInstanceError>),

    #[error("failed to get description of RDS instance")]
    DescribeRDSInstance(#[from] SdkError<DescribeDBInstancesError>),

    #[error["plain error"]]
    Plain(String),
}

unsafe impl Send for Error {}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!(error = &err as &dyn std::error::Error, "provision failed");
        Status::internal("failed to provision a database")
    }
}
