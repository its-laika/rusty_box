use super::error::Error;
use crate::configuration::CONFIGURATION;
use chrono::{Days, Utc};
use migration::ExprTrait;
use sea_orm::sea_query::Query;
use sea_orm::{ColumnTrait, Condition, FromQueryResult};
use sea_orm::{DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set};
use uuid::Uuid;

#[derive(FromQueryResult)]
struct CountResult {
    count: i64,
}

pub async fn get_downloadable_file(
    database_connection: &DatabaseConnection,
    id: &Uuid,
) -> Result<Option<entity::file::Model>, Error> {
    entity::File::find()
        .filter(entity::file::Column::Id.eq(*id))
        .filter(entity::file::Column::DownloadUntil.gte(Utc::now()))
        .filter(
            entity::file::Column::Id.not_in_subquery(
                Query::select()
                    .column(entity::access_log::Column::FileId)
                    .from(entity::access_log::Entity)
                    .cond_where(Condition::all().add(entity::access_log::Column::Successful.eq(1)))
                    .to_owned(),
            ),
        )
        .filter(
            entity::file::Column::Id.not_in_subquery(
                Query::select()
                    .column(entity::access_log::Column::FileId)
                    .from(entity::access_log::Entity)
                    .group_by_col(entity::access_log::Column::FileId)
                    .cond_having(
                        Condition::all().add(
                            entity::access_log::Column::FileId
                                .count()
                                .gte(CONFIGURATION.max_download_tries),
                        ),
                    )
                    .to_owned(),
            ),
        )
        .one(database_connection)
        .await
        .map_err(Error::DatabaseOperationFailed)
}

pub async fn is_upload_limit_reached(
    database_connection: &DatabaseConnection,
    ip: &str,
) -> Result<bool, Error> {
    let min_uploaded_at = Utc::now()
        .checked_sub_days(Days::new(1))
        .ok_or(Error::DateCalculationFailed)?;

    let count = entity::File::find()
        .select_only()
        .column_as(entity::file::Column::Id.count(), "count")
        .filter(entity::file::Column::UploaderIp.eq(ip))
        .filter(entity::file::Column::UploadedAt.gte(min_uploaded_at.naive_utc()))
        .into_model::<CountResult>()
        .one(database_connection)
        .await
        .map_err(Error::DatabaseOperationFailed)?
        .unwrap_or(CountResult { count: 0 })
        .count;

    Ok(count >= CONFIGURATION.ip_uploads_per_day.into())
}

pub async fn store_file(
    database_connection: &DatabaseConnection,
    id: &Uuid,
    hash: String,
    uploader_ip: String,
    encrypted_metadata: Vec<u8>,
) -> Result<(), Error> {
    let now = Utc::now();

    let download_until = now
        .checked_add_days(CONFIGURATION.default_lifetime)
        .ok_or(Error::DateCalculationFailed)?;

    let file = entity::file::ActiveModel {
        id: Set((*id).into()),
        hash: Set(hash),
        uploader_ip: Set(uploader_ip),
        uploaded_at: Set(now.naive_utc()),
        download_until: Set(download_until.naive_utc()),
        encrypted_metadata: Set(encrypted_metadata),
    };

    entity::File::insert(file)
        .exec(database_connection)
        .await
        .map_err(Error::DatabaseOperationFailed)?;

    Ok(())
}

pub async fn store_access_log(
    database_connection: &DatabaseConnection,
    ip: &str,
    file_id: &Uuid,
    successful: bool,
) -> Result<(), Error> {
    let log = entity::access_log::ActiveModel {
        id: Set(Uuid::new_v4().into()),
        ip: Set(ip.into()),
        file_id: Set((*file_id).into()),
        date_time: Set(Utc::now().naive_utc()),
        successful: Set(i8::from(successful)),
    };

    entity::AccessLog::insert(log)
        .exec(database_connection)
        .await
        .map_err(Error::DatabaseOperationFailed)?;

    Ok(())
}
