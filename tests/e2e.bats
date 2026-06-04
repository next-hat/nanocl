#!/usr/bin/env bats

cleanup_statefile() {
  local statefile="$1"
  nanocl state rm -ys "$statefile" >/dev/null 2>&1 || true
}

cleanup_nanocl_artifacts() {
  cleanup_statefile ./examples/deploy_example.yml
  cleanup_statefile ./examples/job_example.yml
  cleanup_statefile ./tests/job_with_error.yml
  nanocl cargo rm -yf test >/dev/null 2>&1 || true
}

setup_file() {
  cleanup_nanocl_artifacts
}

teardown() {
  cleanup_nanocl_artifacts
}

teardown_file() {
  cleanup_nanocl_artifacts
}

@test "nanocl --version" {
  run nanocl --version
  [ "$status" -eq 0 ]
}

@test "nanocl version" {
  run nanocl version
  [ "$status" -eq 0 ]
}

@test "nanocl help" {
  run nanocl help
  [ "$status" -eq 0 ]
}

@test "nanocl info" {
  run nanocl info
  [ "$status" -eq 0 ]
}

@test "nanocl cargo ls" {
  run nanocl cargo ls
  [ "$status" -eq 0 ]
}

@test "nanocl cargo run" {
  run nanocl cargo run test nginx:latest
  [ "$status" -eq 0 ]
}

@test "nanocl cargo rm" {
  run nanocl cargo rm -yf test
  [ "$status" -eq 0 ]
}

@test "nanocl state apply -ys ./examples/deploy_example.yml" {
  run nanocl state apply -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state render -s ./examples/deploy_example.yml" {
  run nanocl state render -s ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}

@test "curl --header \"Host: deploy-example.com\" 127.0.0.1" {
  run sleep 2
  run curl --header "Host: deploy-example.com" 127.0.0.1
  [ "$status" -eq 0 ]
}

@test "nanocl state rm -ys ./examples/deploy_example.yml" {
  run nanocl state rm -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state apply -ys ./examples/job_example.yml" {
  run nanocl state apply -ys ./examples/job_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state rm -ys ./examples/job_example.yml" {
  run nanocl state rm -ys ./examples/job_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state apply fails with invalid statefile" {
  run nanocl state apply -ys ./tests/invalid_statefile.yaml
  [ "$status" -ne 0 ]
}
