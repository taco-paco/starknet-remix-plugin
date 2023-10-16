use crate::handlers::process::{do_process_command, fetch_process_result};
use crate::handlers::types::{ApiCommand, ApiCommandResult, CompileResponse};
use crate::utils::lib::{get_file_ext, get_file_path, CAIRO_DIR, CASM_ROOT};
use crate::worker::WorkerEngine;
use rocket::fs::NamedFile;
use rocket::serde::json;
use rocket::serde::json::Json;
use rocket::tokio::fs;
use rocket::State;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::info;
use tracing::instrument;

#[instrument]
#[get("/compile-to-casm/<remix_file_path..>")]
pub async fn compile_to_casm(remix_file_path: PathBuf) -> Json<CompileResponse> {
    info!("/compile-to-casm/{:?}", remix_file_path);
    do_compile_to_casm(remix_file_path).await
}

#[instrument]
#[get("/compile-to-casm-async/<remix_file_path..>")]
pub async fn compile_to_casm_async(
    remix_file_path: PathBuf,
    engine: &State<WorkerEngine>,
) -> String {
    info!("/compile-to-casm-async/{:?}", remix_file_path);
    do_process_command(ApiCommand::CasmCompile(remix_file_path), engine)
}

#[instrument]
#[get("/compile-to-casm-result/<process_id>")]
pub async fn copmile_to_casm_result(process_id: String, engine: &State<WorkerEngine>) -> String {
    info!("/compile-to-casm-result/{:?}", process_id);
    fetch_process_result(process_id, engine, |result| match result {
        ApiCommandResult::CasmCompile(casm_result) => json::to_string(&casm_result).unwrap(),
        _ => String::from("Result not available"),
    })
}

/// Compile source file to CASM
///
pub async fn do_compile_to_casm(remix_file_path: PathBuf) -> Json<CompileResponse> {
    let remix_file_path = match remix_file_path.to_str() {
        Some(path) => path.to_string(),
        None => {
            return Json(CompileResponse {
                file_content: "".to_string(),
                message: "File path not found".to_string(),
                status: "FileNotFound".to_string(),
            });
        }
    };

    // check if the file has .sierra extension
    match get_file_ext(&remix_file_path) {
        ext if ext == "sierra" => {
            debug!("LOG: File extension is sierra");
        }
        _ => {
            debug!("LOG: File extension not supported");
            return Json(CompileResponse {
                file_content: "".to_string(),
                message: "File extension not supported".to_string(),
                status: "FileExtensionNotSupported".to_string(),
            });
        }
    }

    let file_path = get_file_path(&remix_file_path);

    let casm_remix_path = remix_file_path.replace(&get_file_ext(&remix_file_path), "casm");

    let mut compile = Command::new("cargo");
    compile.current_dir(CAIRO_DIR);

    let casm_path = Path::new(CASM_ROOT).join(&casm_remix_path);

    // create directory for casm file
    match casm_path.parent() {
        Some(parent) => match fs::create_dir_all(parent).await {
            Ok(_) => {
                debug!("LOG: Created directory: {:?}", parent);
            }
            Err(e) => {
                debug!("LOG: Error creating directory: {:?}", e);
            }
        },
        None => {
            debug!("LOG: Error creating directory");
        }
    }

    let result = compile
        .arg("run")
        .arg("--release")
        .arg("--bin")
        .arg("starknet-sierra-compile")
        .arg("--")
        .arg(&file_path)
        .arg(&casm_path)
        .stderr(Stdio::piped())
        .spawn();

    if result.is_err() {
        return Json(CompileResponse {
            file_content: "".to_string(),
            message: "Failed to execute starknet-sierra-compile".to_string(),
            status: "SierraCompilationFailed".to_string(),
        });
    }

    let result = result.unwrap();

    debug!("LOG: ran command:{:?}", compile);

    let output = result.wait_with_output().expect("Failed to wait on child");

    Json(CompileResponse {
        file_content: match NamedFile::open(&casm_path).await.ok() {
            Some(file) => match file.path().to_str() {
                Some(path) => match fs::read_to_string(path.to_string()).await {
                    Ok(casm) => casm.to_string(),
                    Err(e) => e.to_string(),
                },
                None => "".to_string(),
            },
            None => "".to_string(),
        },
        message: String::from_utf8(output.stderr)
            .unwrap()
            .replace(&file_path.to_str().unwrap().to_string(), &remix_file_path)
            .replace(&casm_path.to_str().unwrap().to_string(), &casm_remix_path),
        status: match output.status.code() {
            Some(0) => "Success".to_string(),
            Some(_) => "SierraCompilationFailed".to_string(),
            None => "UnknownError".to_string(),
        },
    })
}
