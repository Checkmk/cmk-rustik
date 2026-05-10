#![allow(dead_code)]
/// A very simple nom parser for the Prometheus sample format.
///
/// Does not try to completely implement the Prometheus or OpenMetrics
/// specification. Mostly, just enough to work with cAdvisor's output.
///
/// In the docstrings of functions, we will use pieces of *this* example
/// to identify which component we are currently parsing:
///
/// `req_secs_created{path="/api/v1",method="GET"} 1605281325.0 1778100948681`
///
/// Note that we do not currently parse exemplars.
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{escaped_transform, tag, tag_no_case},
    character::complete::{
        alpha1, alphanumeric1, char, digit0, digit1, newline, none_of, not_line_ending, one_of,
    },
    combinator::{map, opt, recognize, value},
    multi::{many0, separated_list0},
    sequence::{delimited, pair, preceded, separated_pair, terminated},
};
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
struct Sample {
    metric_name: String,
    labels: HashMap<String, String>,
    value: String,
    timestamp: Option<String>,
}

impl Sample {
    pub fn new(
        metric_name: String,
        labels: HashMap<String, String>,
        value: String,
        timestamp: Option<String>,
    ) -> Self {
        Sample {
            metric_name,
            labels,
            value,
            timestamp,
        }
    }
}

/// Parse what the spec calls a "normal" character.
///
/// > Any unicode character, except newline, double quote, and backslash
///
/// From the sample above, any individual character of `/api/v1` or `GET`,
/// for example.
fn normal_char(input: &str) -> IResult<&str, char> {
    none_of("\n\"\\").parse(input)
}

/// Parse a label name.
///
/// From the sample above, `path` or `method`
fn label_name(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0(alt((alphanumeric1, tag("_")))),
    ))
    .parse(input)
}

/// Parse a label value.
///
/// From the sample above, `"/api/v1"` or `"GET"`
///
/// Similar to the official Go parser, and contrary to what the specification
/// technically allows, we do NOT permit escape literals (e.g. preserving `\a`
/// where `a` is any character other than `\`, `"`, or `n`). The Go parser
/// explicitly errors in this case, and we, too, allow the parse to fail.
fn label_value(input: &str) -> IResult<&str, String> {
    let transform = alt((
        value("\\", char('\\')),
        value("\"", char('"')),
        value("\n", char('n')),
        // There's no good way to fallback and allow \a -> \a for 'a' not in the
        // list above. Thus we follow what the official Go parser does and just
        // not permit it.
    ));
    delimited(
        char('"'),
        opt(escaped_transform(normal_char, '\\', transform)).map(|o| o.unwrap_or_default()),
        char('"'),
    )
    .parse(input)
}

/// Parse a particular label.
///
/// From the sample above, `path="/api/v1"`
fn label(input: &str) -> IResult<&str, (&str, String)> {
    separated_pair(label_name, char('='), label_value).parse(input)
}

/// Parse a set of labels enclosed in `{`...`}`
///
/// From the sample above, `{path="/api/v1",method="GET"}`
fn labels(input: &str) -> IResult<&str, HashMap<String, String>> {
    let labels = delimited(char('{'), separated_list0(char(','), label), char('}'));

    map(labels, |vec| {
        vec.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    })
    .parse(input)
}

/// Parse a metric name.
///
/// From the sample above, `req_secs_created`
///
/// It can start with a letter, underscore, or colon, and can contain any of those + digits.
fn metric_name(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        alt((alpha1, tag("_"), tag(":"))),
        many0(alt((alphanumeric1, tag("_"), tag(":")))),
    ))
    .parse(input)
}

/// Parse a sign (`+` or `-`).
fn sign(input: &str) -> IResult<&str, char> {
    one_of("+-").parse(input)
}

/// Parse a real number.
///
/// From the sample above, `1605281325.0` or `1778100948681`.
///
/// Other examples include `1e-10`, `1e+10`, `1e10`, `000000004`.
fn realnumber(input: &str) -> IResult<&str, &str> {
    let exponential = || opt((one_of("eE"), opt(one_of("+-")), digit1));
    let decimal_after = recognize((opt(sign), digit1, opt((char('.'), digit0)), exponential()));
    let decimal_before = recognize((opt(sign), digit0, char('.'), digit1, exponential()));
    alt((decimal_after, decimal_before)).parse(input)
}

/// Parse a number.
///
/// From the sample above, `1605281325.0` or `1778100948681`.
///
/// Other examples include _all real numbers_ but also "infinity", "inf", and "nan"
/// case insensitively.
fn number(input: &str) -> IResult<&str, &str> {
    let infinity = recognize((
        opt(sign),
        alt((tag_no_case("infinity"), tag_no_case("inf"))), // order matters here
    ));
    alt((realnumber, infinity, tag_no_case("nan"))).parse(input)
}

/// Parse a sample.
///
/// From the sample above,
/// `req_secs_created{path="/api/v1",method="GET"} 1605281325.0 1778100948681`.
///
/// Does *not* parse a trailing newline. Use [`sample_line`] for that.
fn sample(input: &str) -> IResult<&str, Sample> {
    let sample_parser = (
        metric_name,
        opt(labels),
        preceded(char(' '), number),
        opt(preceded(char(' '), realnumber)),
    );
    map(sample_parser, |(metric_name, labels, value, timestamp)| {
        Sample::new(
            metric_name.to_string(),
            labels.unwrap_or_default(),
            value.to_string(),
            timestamp.map(|t| t.to_string()),
        )
    })
    .parse(input)
}

/// Parse a sample with trailing newline.
fn sample_line(input: &str) -> IResult<&str, Sample> {
    terminated(sample, newline).parse(input)
}

/// Parse (drop) a metric descriptor.
///
/// More loosely, we drop any line beginning with `#`.
fn metric_descriptor(input: &str) -> IResult<&str, ()> {
    value((), preceded(char('#'), not_line_ending)).parse(input)
}

fn empty_line(input: &str) -> IResult<&str, Option<Sample>> {
    Ok((input, None))
}

/// Parse a full exposition line including its newline.
fn exposition_line(input: &str) -> IResult<&str, Option<Sample>> {
    terminated(
        alt((
            sample.map(Some),
            metric_descriptor.map(|_| None),
            |i| Ok((i, None)), // Empty line
        )),
        newline,
    )
    .parse(input)
}

/// Parse many exposition lines.
fn exposition_lines(input: &str) -> IResult<&str, Vec<Sample>> {
    many0(exposition_line)
        .map(|opts| opts.into_iter().flatten().collect())
        .parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nom::{
        Err,
        combinator::all_consuming,
        error::{Error, ErrorKind},
    };

    #[test]
    fn test_label_name() {
        // Must start with a letter or underscore
        assert_eq!(label_name("path"), Ok(("", "path")));
        assert_eq!(label_name("_path"), Ok(("", "_path")));

        // Cannot be empty
        assert_eq!(
            label_name(""),
            Err(Err::Error(Error::new("", ErrorKind::Tag)))
        );

        // Cannot start with some random symbol
        assert_eq!(
            label_name("="),
            Err(Err::Error(Error::new("=", ErrorKind::Tag)))
        );

        // Stops consuming after some other symbol
        assert_eq!(label_name("foo=asdf"), Ok(("=asdf", "foo")));
        assert_eq!(label_name("foo\"asdf"), Ok(("\"asdf", "foo")));

        // Cannot start with a number
        assert_eq!(
            label_name("1337path"),
            Err(Err::Error(Error::new("1337path", ErrorKind::Tag)))
        );

        // But can contain a number
        assert_eq!(label_name("foo_3"), Ok(("", "foo_3")));
    }

    #[test]
    fn test_label_value() {
        assert_eq!(label_value("\"\""), Ok(("", "".to_string())));
        assert_eq!(label_value("\"foo\""), Ok(("", "foo".to_string())));
        assert_eq!(
            label_value("\"s p a  \t c e s \""),
            Ok(("", "s p a  \t c e s ".to_string()))
        );
        assert_eq!(label_value("\"foo\\n\""), Ok(("", "foo\n".to_string())));
        assert_eq!(
            label_value("\"foo\\n\\\"hey\\\"\""),
            Ok(("", "foo\n\"hey\"".to_string()))
        );
        assert_eq!(label_value("\"foo\"\n"), Ok(("\n", "foo".to_string())));
        assert_eq!(
            label_value("\"foo \\a bar"),
            Err(Err::Error(Error::new("foo \\a bar", ErrorKind::Char)))
        );
    }

    #[test]
    fn test_label() {
        assert_eq!(
            label("path=\"/api/v1\""),
            Ok(("", ("path", "/api/v1".to_string())))
        );

        assert_eq!(
            label("path=/api/v1"),
            Err(Err::Error(Error::new("/api/v1", ErrorKind::Char)))
        );
    }

    #[test]
    fn test_labels() {
        let mut expected = HashMap::new();
        expected.insert("path".to_string(), "/api/v1".to_string());
        expected.insert("method".to_string(), "GET".to_string());

        assert_eq!(
            labels("{path=\"/api/v1\",method=\"GET\"}"),
            Ok(("", expected))
        );

        // Can have no labels
        assert_eq!(labels("{}"), Ok(("", HashMap::new())));
    }

    #[test]
    fn test_metric_name() {
        assert_eq!(
            metric_name("req_secs_created"),
            Ok(("", "req_secs_created"))
        );
        assert_eq!(
            metric_name("_req_secs_created"),
            Ok(("", "_req_secs_created"))
        );
        assert_eq!(
            metric_name(":req_secs_created"),
            Ok(("", ":req_secs_created"))
        );

        // Cannot be empty
        assert_eq!(
            metric_name(""),
            Err(Err::Error(Error::new("", ErrorKind::Tag)))
        );

        // Cannot start with some random symbol
        assert_eq!(
            metric_name("="),
            Err(Err::Error(Error::new("=", ErrorKind::Tag)))
        );

        // Stops consuming after some other symbol
        assert_eq!(metric_name("foo=asdf"), Ok(("=asdf", "foo")));
        assert_eq!(metric_name("foo\"asdf"), Ok(("\"asdf", "foo")));

        // Cannot start with a number
        assert_eq!(
            metric_name("1337req_secs_created"),
            Err(Err::Error(Error::new(
                "1337req_secs_created",
                ErrorKind::Tag
            )))
        );

        // But can contain a number
        assert_eq!(metric_name("foo_3"), Ok(("", "foo_3")));

        // Importantly, it stops after spaces or "{"
        assert_eq!(metric_name("foo bar"), Ok((" bar", "foo")));
        assert_eq!(metric_name("foo{"), Ok(("{", "foo")));
    }

    #[test]
    fn test_realnumber() {
        assert_eq!(realnumber("3"), Ok(("", "3")));
        assert_eq!(realnumber("31"), Ok(("", "31")));
        assert_eq!(realnumber("1."), Ok(("", "1.")));
        assert_eq!(realnumber("123."), Ok(("", "123.")));
        assert_eq!(realnumber(".1"), Ok(("", ".1")));
        assert_eq!(realnumber(".123"), Ok(("", ".123")));
        assert_eq!(realnumber("000031"), Ok(("", "000031")));
        assert_eq!(realnumber("31.3e6"), Ok(("", "31.3e6")));
        assert_eq!(realnumber("2e1"), Ok(("", "2e1")));
        assert_eq!(realnumber("2e10"), Ok(("", "2e10")));
        assert_eq!(realnumber("+8"), Ok(("", "+8")));
        assert_eq!(realnumber("+33.3"), Ok(("", "+33.3")));
        assert_eq!(realnumber("+33.3e32"), Ok(("", "+33.3e32")));
        assert_eq!(realnumber("-000019.0003"), Ok(("", "-000019.0003")));
        assert_eq!(realnumber("+000019.0003"), Ok(("", "+000019.0003")));
        assert_eq!(realnumber("-19.0003"), Ok(("", "-19.0003")));
        assert_eq!(realnumber("-19."), Ok(("", "-19.")));
        assert_eq!(realnumber("-19.e2"), Ok(("", "-19.e2")));
        assert_eq!(realnumber("-.9e0003"), Ok(("", "-.9e0003")));
        assert_eq!(realnumber("-.9"), Ok(("", "-.9")));
        assert_eq!(realnumber("1E10"), Ok(("", "1E10")));
        assert_eq!(realnumber("0"), Ok(("", "0")));
        assert_eq!(realnumber("0.0"), Ok(("", "0.0")));
        assert_eq!(realnumber("-0"), Ok(("", "-0")));
        // Negative cases
        assert_eq!(
            realnumber(""),
            Err(Err::Error(Error::new("", ErrorKind::Char)))
        );
        assert_eq!(
            realnumber("+"),
            Err(Err::Error(Error::new("", ErrorKind::Char)))
        );
        assert_eq!(
            realnumber("-"),
            Err(Err::Error(Error::new("", ErrorKind::Char)))
        );
        assert_eq!(
            realnumber("."),
            Err(Err::Error(Error::new("", ErrorKind::Digit)))
        );
        assert_eq!(
            realnumber("-."),
            Err(Err::Error(Error::new("", ErrorKind::Digit)))
        );
        assert_eq!(
            realnumber("e10"),
            Err(Err::Error(Error::new("e10", ErrorKind::Char)))
        );
        assert_eq!(
            realnumber(".e10"),
            Err(Err::Error(Error::new("e10", ErrorKind::Digit)))
        );
        assert_eq!(
            realnumber("-.e10"),
            Err(Err::Error(Error::new("e10", ErrorKind::Digit)))
        );
        // These all parse a valid prefix and leave the rest in the input;
        // wrap with all_consuming to assert "this whole string is invalid".
        assert_eq!(
            all_consuming(realnumber).parse("1e"),
            Err(Err::Error(Error::new("e", ErrorKind::Eof)))
        );
        assert_eq!(
            all_consuming(realnumber).parse("1e-"),
            Err(Err::Error(Error::new("e-", ErrorKind::Eof)))
        );
        assert_eq!(
            all_consuming(realnumber).parse("1e+"),
            Err(Err::Error(Error::new("e+", ErrorKind::Eof)))
        );
        assert_eq!(
            all_consuming(realnumber).parse("1e-10.5"),
            Err(Err::Error(Error::new(".5", ErrorKind::Eof)))
        );
        assert_eq!(
            all_consuming(realnumber).parse("1e+10.5"),
            Err(Err::Error(Error::new(".5", ErrorKind::Eof)))
        );
        assert_eq!(
            all_consuming(realnumber).parse("1.2.3"),
            Err(Err::Error(Error::new(".3", ErrorKind::Eof)))
        );
        assert_eq!(
            all_consuming(realnumber).parse("1e10.5"),
            Err(Err::Error(Error::new(".5", ErrorKind::Eof)))
        );
    }

    #[test]
    fn test_number() {
        assert_eq!(number("3"), Ok(("", "3")));
        assert_eq!(number("31"), Ok(("", "31")));
        assert_eq!(number("inf"), Ok(("", "inf")));
        assert_eq!(number("Inf"), Ok(("", "Inf")));
        assert_eq!(number("infinity"), Ok(("", "infinity")));
        assert_eq!(number("InfInItY"), Ok(("", "InfInItY")));
        assert_eq!(number("INFINITY"), Ok(("", "INFINITY")));
        assert_eq!(number("NaN"), Ok(("", "NaN")));
        assert_eq!(number("nan"), Ok(("", "nan")));
        assert_eq!(number("NAN"), Ok(("", "NAN")));
    }

    #[test]
    fn test_sample() {
        let mut labels = HashMap::new();
        labels.insert("path".to_string(), "/api/v1".to_string());
        labels.insert("method".to_string(), "GET".to_string());
        assert_eq!(
            sample("req_secs_created{path=\"/api/v1\",method=\"GET\"} 1605281325.0 1778100948681"),
            Ok((
                "",
                Sample::new(
                    "req_secs_created".to_string(),
                    labels.clone(),
                    "1605281325.0".to_string(),
                    Some("1778100948681".to_string()),
                )
            ))
        );
        assert_eq!(
            sample("life_the_universe_and_everything 42"),
            Ok((
                "",
                Sample::new(
                    "life_the_universe_and_everything".to_string(),
                    HashMap::new(),
                    "42".to_string(),
                    None,
                )
            ))
        );
        // Does not consume trailing newline
        assert_eq!(
            sample(
                "req_secs_created{path=\"/api/v1\",method=\"GET\"} 1605281325.0 1778100948681\n"
            ),
            Ok((
                "\n",
                Sample::new(
                    "req_secs_created".to_string(),
                    labels,
                    "1605281325.0".to_string(),
                    Some("1778100948681".to_string()),
                )
            ))
        );
    }

    #[test]
    fn test_sample_line() {
        let mut labels = HashMap::new();
        labels.insert("path".to_string(), "/api/v1".to_string());
        labels.insert("method".to_string(), "GET".to_string());
        assert_eq!(
            sample_line(
                "req_secs_created{path=\"/api/v1\",method=\"GET\"} 1605281325.0 1778100948681\n"
            ),
            Ok((
                "",
                Sample::new(
                    "req_secs_created".to_string(),
                    labels.clone(),
                    "1605281325.0".to_string(),
                    Some("1778100948681".to_string()),
                )
            ))
        );
    }

    #[test]
    fn test_metric_descriptor() {
        assert_eq!(metric_descriptor("#foo bar\n"), Ok(("\n", ())));
        assert_eq!(metric_descriptor("# foo bar\n"), Ok(("\n", ())));
        assert_eq!(metric_descriptor("#\n"), Ok(("\n", ())));
        assert_eq!(metric_descriptor("#"), Ok(("", ())));
        assert_eq!(metric_descriptor("# blip"), Ok(("", ())));
        assert_eq!(metric_descriptor("#blip"), Ok(("", ())));
    }

    #[test]
    fn test_exposition_line() {
        let mut labels = HashMap::new();
        labels.insert("path".to_string(), "/api/v1".to_string());
        labels.insert("method".to_string(), "GET".to_string());
        assert_eq!(
            exposition_line(
                "req_secs_created{path=\"/api/v1\",method=\"GET\"} 1605281325.0 1778100948681\n"
            ),
            Ok((
                "",
                Some(Sample::new(
                    "req_secs_created".to_string(),
                    labels.clone(),
                    "1605281325.0".to_string(),
                    Some("1778100948681".to_string()),
                ))
            ))
        );
        assert_eq!(
            exposition_line("life_the_universe_and_everything 42\n"),
            Ok((
                "",
                Some(Sample::new(
                    "life_the_universe_and_everything".to_string(),
                    HashMap::new(),
                    "42".to_string(),
                    None,
                ))
            ))
        );
        assert_eq!(exposition_line("# foo\n"), Ok(("", None)));
        assert_eq!(exposition_line("\n"), Ok(("", None)));
        assert_eq!(
            exposition_line("bananas"),
            Err(Err::Error(Error::new("bananas", ErrorKind::Char)))
        );
    }

    #[test]
    fn test_exposition_lines() -> Result<(), Err<Error<&'static str>>> {
        let cadvisor = include_str!("../tests/fixtures/cadvisor");
        let (remaining, parsed) = exposition_lines(cadvisor)?;
        assert_eq!(remaining, "");
        assert_eq!(parsed.len(), 952);
        Ok(())
    }
}
