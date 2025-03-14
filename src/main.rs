use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::{Pool, Sqlite, SqlitePool};
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[derive(Debug)]
enum AppError {
    NotFound,
    DatabaseError(sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "Error": "User not found" })),
            ),
            AppError::DatabaseError(err) => {
                eprintln!("Database error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "Error": "Database error" })),
                )
            }
        }
        .into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::DatabaseError(err)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Clone)]
struct AppState {
    db: Pool<Sqlite>,
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    dotenv().ok();

    let db = SqlitePool::connect(&std::env::var("DATABASE_URL").unwrap()).await?;

    sqlx::migrate!().run(&db).await?;

    let app = Router::new()
        .route("/", get(root))
        .route("/users", get(get_users).post(create_user))
        .route("/users/{id}", get(get_user_by_id).delete(delete_user))
        .with_state(AppState { db });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🚀 Server running at http://{}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn root() -> &'static str {
    "Hello, Axum with SQLite!"
}

async fn get_users(State(state): State<AppState>) -> Result<Json<Vec<User>>, AppError> {
    let users = sqlx::query_as::<_, User>("SELECT * FROM users")
        .fetch_all(&state.db)
        .await
        .map_err(AppError::from)?;

    Ok(Json(users))
}

async fn get_user_by_id(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<User>, AppError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::from)?;

    match user {
        Some(user) => Ok(Json(user)),
        None => Err(AppError::NotFound),
    }
}

async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUser>,
) -> Result<Json<User>, AppError> {
    let result = sqlx::query("INSERT INTO users (name, email) VALUES (?, ?)")
        .bind(&payload.name)
        .bind(&payload.email)
        .execute(&state.db)
        .await
        .map_err(AppError::from)?;

    let new_user = User {
        id: result.last_insert_rowid(),
        name: payload.name,
        email: payload.email,
    };

    Ok(Json(new_user))
}

async fn delete_user(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(AppError::from)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
