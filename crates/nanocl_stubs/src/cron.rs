//! Parsing and validation of job schedules for the BusyBox cron environment.

/// Validate a single cron schedule without a command or environment directive.
///
/// Accepts five ASCII-space-separated fields: minute (0-59), hour (0-23),
/// day of month (1-31), month (1-12), and day of week (0-6, Sunday is 0).
/// Fields support `*`, comma lists, ranges, and positive `/step` suffixes.
/// Month and weekday values may be case-insensitive three-letter names.
/// As in BusyBox, ranges may wrap and a step on a single value does not
/// implicitly extend that value to the end of the field.
///
/// Also accepts `@yearly`, `@annually`, `@monthly`, `@weekly`, `@daily`,
/// `@midnight`, `@hourly`, and `@reboot`. Control characters (including tabs,
/// CR and LF), non-ASCII characters, and additional tokens are rejected.
/// The original schedule is never trimmed or sanitized before this check.
pub fn validate_schedule(schedule: &str) -> Result<(), String> {
  // Check before splitting or trimming: whitespace helpers can hide newlines.
  if !schedule.is_ascii()
    || schedule.bytes().any(|byte| byte.is_ascii_control())
  {
    return Err(
      "cron schedule must contain only printable ASCII characters".to_owned(),
    );
  }
  let schedule = schedule.trim_matches(' ');
  if schedule.starts_with('@') {
    return match schedule {
      "@yearly" | "@annually" | "@monthly" | "@weekly" | "@daily"
      | "@midnight" | "@hourly" | "@reboot" => Ok(()),
      _ => Err("invalid cron schedule shortcut".to_owned()),
    };
  }

  let mut fields = schedule.split(' ').filter(|field| !field.is_empty());
  let months = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct",
    "nov", "dec",
  ];
  let weekdays = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
  for (name, min, max, names) in [
    ("minute", 0, 59, &[][..]),
    ("hour", 0, 23, &[][..]),
    ("day of month", 1, 31, &[][..]),
    ("month", 1, 12, &months[..]),
    ("day of week", 0, 6, &weekdays[..]),
  ] {
    let field = fields.next().ok_or_else(|| {
      "cron schedule must contain exactly five fields".to_owned()
    })?;
    validate_field(field, min, max, names)
      .map_err(|err| format!("invalid cron {name}: {err}"))?;
  }
  if fields.next().is_some() {
    return Err("cron schedule must contain exactly five fields".to_owned());
  }
  Ok(())
}

fn validate_field(
  field: &str,
  min: u8,
  max: u8,
  names: &[&str],
) -> Result<(), &'static str> {
  for item in field.split(',') {
    let range = match item.split_once('/') {
      Some((range, step)) => {
        // BusyBox stores the step in a signed C int.
        if !step.bytes().all(|byte| byte.is_ascii_digit())
          || !matches!(step.parse::<i32>(), Ok(1..=i32::MAX))
        {
          return Err("step must be an integer between 1 and 2147483647");
        }
        range
      }
      None => item,
    };
    if range == "*" {
      continue;
    }
    match range.split_once('-') {
      Some((start, end)) => {
        validate_value(start, min, max, names)?;
        validate_value(end, min, max, names)?;
      }
      None => validate_value(range, min, max, names)?,
    }
  }
  Ok(())
}

fn validate_value(
  value: &str,
  min: u8,
  max: u8,
  names: &[&str],
) -> Result<(), &'static str> {
  if names.iter().any(|name| value.eq_ignore_ascii_case(name)) {
    return Ok(());
  }
  if value.bytes().all(|byte| byte.is_ascii_digit())
    && value
      .parse::<u8>()
      .is_ok_and(|value| (min..=max).contains(&value))
  {
    return Ok(());
  }
  Err("expected an in-range number or supported three-letter name")
}

#[cfg(test)]
mod tests {
  use super::validate_schedule;

  #[test]
  fn accepts_busybox_cron_expressions() {
    for schedule in [
      "* * * * *",
      "*/1 * * * *",
      "0 0 1 1 0",
      "59 23 31 12 6",
      "*/15 0-23/2 1,15 * 1-5",
      "0,15,30,45 9-17 * JAN,Jun,dec MON-FRI",
      "0 0 * jan-dec/2 sun,sat",
      "50-10/2 22-2 28-3 nov-feb fri-mon",
      "5/10 * * * *",
      "*/60 * * * *",
      "*/2147483647 * * * *",
      "  0  0  *  *  *  ",
    ] {
      assert!(validate_schedule(schedule).is_ok(), "{schedule:?}");
    }
  }

  #[test]
  fn accepts_only_complete_supported_shortcuts() {
    for shortcut in [
      "@yearly",
      "@annually",
      "@monthly",
      "@weekly",
      "@daily",
      "@midnight",
      "@hourly",
      "@reboot",
    ] {
      assert!(validate_schedule(shortcut).is_ok());
      assert!(validate_schedule(&format!(" {shortcut} ")).is_ok());
      assert!(validate_schedule(&format!("{shortcut} echo injected")).is_err());
    }
    for schedule in ["@", "@DAILY", "@every 1h", "@minutely", "@daily#comment"]
    {
      assert!(validate_schedule(schedule).is_err(), "{schedule:?}");
    }
  }

  #[test]
  fn rejects_crontab_injection_and_extra_fields() {
    for schedule in [
      "* * * * *\n* * * * * touch /tmp/injected",
      "* * * * *\r* * * * * touch /tmp/injected",
      "* * * * *\r\nSHELL=/bin/sh",
      "* * * * * echo injected",
      "* * * * *;echo injected",
      "* * * * *#comment",
      "* * * * * # comment",
      "SHELL=/bin/sh",
      "MAILTO=root",
      "# * * * *",
      "0 0 * * $(id)",
      "0 0 * * `id`",
      "0 0 * * %id",
      "0 0 * * *\\",
      "",
      " ",
      "* * * *",
      "* * * * * *",
      "0 0 0 * * * *",
    ] {
      assert!(validate_schedule(schedule).is_err(), "{schedule:?}");
    }
  }

  #[test]
  fn rejects_controls_before_whitespace_handling() {
    for control in (0..=127u8).filter(u8::is_ascii_control) {
      let control = char::from(control);
      for schedule in [
        format!("{control}* * * * *"),
        format!("* * * * *{control}"),
        format!("*{control}* * * *"),
        format!("@daily{control}"),
      ] {
        assert!(validate_schedule(&schedule).is_err(), "{schedule:?}");
      }
    }
    for character in ['\u{85}', '\u{a0}', '\u{2028}', '\u{2029}', '１', 'é'] {
      let schedule = format!("* * * * *{character}");
      assert!(validate_schedule(&schedule).is_err(), "{schedule:?}");
    }
  }

  #[test]
  fn rejects_invalid_field_bounds_and_names() {
    for schedule in [
      "60 * * * *",
      "* 24 * * *",
      "* * 0 * *",
      "* * 32 * *",
      "* * * 0 *",
      "* * * 13 *",
      "* * * * 7",
      "* * * * 8",
      "0-60 * * * *",
      "* 1,24 * * *",
      "* * 1-32 * *",
      "* * * jan-13 *",
      "* * * * mon-7",
      "JAN * * * *",
      "* * MON * *",
      "* * * MON *",
      "* * * * JAN",
      "* * * january *",
      "* * * * monday",
    ] {
      assert!(validate_schedule(schedule).is_err(), "{schedule:?}");
    }
  }

  #[test]
  fn rejects_malformed_lists_ranges_steps_and_overflow() {
    for field in [
      ",",
      ",1",
      "1,",
      "1,,2",
      "-1",
      "+1",
      "1-",
      "-",
      "1--2",
      "1-2-3",
      "*-2",
      "1-*",
      "**",
      "?",
      "1L",
      "1#2",
      "*/0",
      "*/-1",
      "*/+1",
      "*/",
      "/2",
      "1-2/0",
      "1/2/3",
      "*/2147483648",
      "99999999999999999999",
      "*/99999999999999999999",
      "1/2,",
      "1.5",
      "1/2.5",
    ] {
      let schedule = format!("{field} * * * *");
      assert!(validate_schedule(&schedule).is_err(), "{schedule:?}");
    }
  }
}
