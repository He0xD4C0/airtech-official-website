use std::{collections::BTreeSet, env, fmt, net::IpAddr, str::FromStr};

use base64::{engine::general_purpose, Engine as _};
use thiserror::Error;

use crate::services::media::MediaSettings;
