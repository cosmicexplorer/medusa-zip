/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

//! ???

pub(crate) mod clap_handlers {
  use clap::error::{ContextKind, ContextValue, ErrorKind};

  pub fn prepare_clap_error<R: AsRef<str>>(
    cmd: &clap::Command,
    arg: Option<&clap::Arg>,
    val: R,
  ) -> clap::Error {
    let mut err = clap::Error::new(ErrorKind::ValueValidation).with_cmd(cmd);
    if let Some(arg) = arg {
      err.insert(
        ContextKind::InvalidArg,
        ContextValue::String(arg.to_string()),
      );
    }
    err.insert(
      ContextKind::InvalidValue,
      ContextValue::String(val.as_ref().to_string()),
    );
    err
  }

  /* NB: These are the only way the default clap formatter will print out any
   * additional context. It is ridiculously frustrating. */
  pub fn process_clap_error(err: &mut clap::Error, e: impl std::fmt::Display, msg: &str) {
    err.insert(
      ContextKind::Usage,
      ContextValue::StyledStr(format!("Error: {}.", e).into()),
    );
    err.insert(
      ContextKind::Suggested,
      ContextValue::StyledStrs(vec![msg.to_string().into()]),
    );
  }
}
