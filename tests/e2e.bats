#!/usr/bin/env bats

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

@test "curl --header \"Host: deploy-example.com\" 127.0.0.1" {
  run sleep 1
  run curl --header "Host: deploy-example.com" 127.0.0.1
  [ "$status" -eq 0 ]
}

@test "nanocl state rm -ys ./examples/deploy_example.yml" {
  run nanocl state rm -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}
