// Copyright 2019 Cargill Incorporated
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashMap;

use actix_web::{AsyncResponder, HttpMessage, HttpRequest, HttpResponse, Query, State};
use futures::future;
use futures::future::Future;
use sawtooth_sdk::messages::batch::BatchList;
use serde::Deserialize;

use crate::rest_api::{error::RestApiResponseError, AppState};
use crate::submitter::{BatchStatusResponse, BatchStatuses, SubmitBatches, DEFAULT_TIME_OUT};

pub fn submit_batches(
    (req, state): (HttpRequest<AppState>, State<AppState>),
) -> impl Future<Item = HttpResponse, Error = RestApiResponseError> {
    req.body().from_err().and_then(
        move |body| -> Box<dyn Future<Item = HttpResponse, Error = RestApiResponseError>> {
            let batch_list: BatchList = match protobuf::parse_from_bytes(&*body) {
                Ok(batch_list) => batch_list,
                Err(err) => {
                    return Box::new(future::err(RestApiResponseError::BadRequest(format!(
                        "Protobuf message was badly formatted. {}",
                        err.to_string()
                    ))));
                }
            };
            let response_url = match req.url_for_static("batch_statuses") {
                Ok(url) => url,
                Err(err) => return Box::new(future::err(err.into())),
            };

            match state.batch_submitter.submit_batches(SubmitBatches {
                batch_list,
                response_url,
            }) {
                Ok(link) => Box::new(future::ok(HttpResponse::Ok().json(link))),
                Err(err) => Box::new(future::err(err)),
            }
        },
    )
}

#[derive(Deserialize, Debug)]
struct Params {
    id: Vec<String>,
}

pub fn get_batch_statuses(
    (state, query, req): (
        State<AppState>,
        Query<HashMap<String, String>>,
        HttpRequest<AppState>,
    ),
) -> Box<dyn Future<Item = HttpResponse, Error = RestApiResponseError>> {
    let batch_ids = match query.get("id") {
        Some(ids) => ids.split(',').map(ToString::to_string).collect(),
        None => {
            return future::err(RestApiResponseError::BadRequest(
                "Request for statuses missing id query.".to_string(),
            ))
            .responder();
        }
    };

    // Max wait time allowed is 95% of network's configured timeout
    let max_wait_time = (DEFAULT_TIME_OUT * 95) / 100;

    let wait = match query.get("wait") {
        Some(wait_time) => {
            if wait_time == "false" {
                None
            } else {
                match wait_time.parse::<u32>() {
                    Ok(wait_time) => {
                        if wait_time > max_wait_time {
                            Some(max_wait_time)
                        } else {
                            Some(wait_time)
                        }
                    }
                    Err(_) => {
                        return future::err(RestApiResponseError::BadRequest(format!(
                            "Query wait has invalid value {}. \
                             It should set to false or a a time in seconds to wait for the commit",
                            wait_time
                        )))
                        .responder();
                    }
                }
            }
        }

        None => Some(max_wait_time),
    };

    let response_url = match req.url_for_static("batch_statuses") {
        Ok(url) => format!("{}?{}", url, req.query_string()),
        Err(err) => return Box::new(future::err(err.into())),
    };

    match state
        .batch_submitter
        .batch_status(BatchStatuses { batch_ids, wait })
    {
        Ok(batch_statuses) => Box::new(future::ok(HttpResponse::Ok().json(BatchStatusResponse {
            data: batch_statuses,
            link: response_url,
        }))),
        Err(err) => Box::new(future::err(err)),
    }
}
