/// The scheduler is a stateless service
/// Using the nanocl client, the scheduler watch for metrics and schedule container to run on the node
/// The scheduler is responsible for scheduling the container to run on the node
/// It use the gossip algorithm to communicate with other nodes
/// To create the neighbor list, the scheduler use latency to determine the distance between nodes
///
use std::sync::{Arc, Mutex};

struct Node {
  hostname: String,
  ip_addr: String,
  port: u16,
}

#[ntex::main]
async fn main() {
  println!("Hello, world!");

  let node = Node {
    hostname: "localhost".to_string(),
    ip_addr: "127.0.0.1".to_string(),
    port: 8080,
  };

  let nodes = vec![node];
  let neighbor_list: Arc<Mutex<Vec<Node>>> = Arc::new(Mutex::new(vec![]));

  // ping other nodes to determine latency, and create the neighbor list
  // The neighbor list is a list of nodes that are close to the current node
  // The maximum list of neighbors is 5
  for node in nodes {
    // ping the node
    // if the node is reachable, add it to the neighbor list
    // if the neighbor list is full, remove the node with the highest latency
    // add the new node to the neighbor list
    let addr = format!("http://{}:{}", node.ip_addr, node.port);
    let client = ntex::http::client::Client::default();
    let now = std::time::Instant::now();
    let response = client.get(addr).send().await.unwrap();
    let latency = now.elapsed().as_millis();
    println!("Latency: {}", latency);
    if neighbor_list.lock().unwrap().len() < 5 {
      neighbor_list.lock().unwrap().push(node);
    } else {
      // remove the node with the highest latency
      let mut max_latency = 0;
      let mut max_index = 0;
      for (index, node) in neighbor_list.lock().unwrap().iter().enumerate() {
        if latency > max_latency {
          max_latency = latency;
          max_index = index;
        }
      }
      neighbor_list.lock().unwrap().remove(max_index);
      neighbor_list.lock().unwrap().push(node);
    }
  }
}
