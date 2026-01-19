//! Downstream Kafka Consumer Example
//!
//! This example demonstrates how to use the XZepr consumer to:
//! 1. Connect to Kafka with SASL/SCRAM authentication
//! 2. Consume CloudEvents messages from XZepr
//! 3. Process deployment events
//! 4. Post work lifecycle events back to XZepr
//!
//! # Running
//!
//! Set the required environment variables:
//! ```bash
//! export XZEPR_KAFKA_BROKERS="kafka.example.com:9093"
//! export XZEPR_KAFKA_TOPIC="xzepr.events"
//! export XZEPR_KAFKA_SECURITY_PROTOCOL="SASL_SSL"
//! export XZEPR_KAFKA_SASL_USERNAME="your-username"
//! export XZEPR_KAFKA_SASL_PASSWORD="your-password"
//! export XZEPR_API_URL="http://xzepr.example.com:8042"
//! export XZEPR_API_TOKEN="your-jwt-token"
//! ```
//!
//! Then run with:
//! ```bash
//! cargo run --example downstream_consumer
//! ```

use std::sync::Arc;
use xzatoma::xzepr::consumer::{
    CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprClient, XzeprConsumer,
};

/// Service name for this downstream consumer.
const SERVICE_NAME: &str = "deployment-handler";

/// Event receiver type for this service.
const RECEIVER_TYPE: &str = "worker";

/// Version of this service.
const VERSION: &str = "1.0.0";

/// Handler for processing deployment events from XZepr.
struct DeploymentHandler {
    /// XZepr API client for posting lifecycle events.
    client: XzeprClient,
    /// Event receiver ID for this service.
    receiver_id: String,
}

impl DeploymentHandler {
    /// Creates a new deployment handler.
    ///
    /// This will discover or create an event receiver for this service.
    async fn new(client: XzeprClient) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Discover or create event receiver for this service
        let receiver_id = client
            .discover_or_create_event_receiver(
                &format!("{}-receiver", SERVICE_NAME),
                RECEIVER_TYPE,
                VERSION,
                &format!("Event receiver for {} downstream service", SERVICE_NAME),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "deployment_id": {"type": "string"},
                        "status": {"type": "string"}
                    }
                }),
            )
            .await?;

        println!("Using event receiver: {}", receiver_id);
        Ok(Self {
            client,
            receiver_id,
        })
    }

    /// Processes a deployment event.
    async fn process_deployment(
        &self,
        event: &CloudEventMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let work_id = &event.id;
        let work_name = "deployment";

        println!(
            "Processing deployment event: {} (type: {})",
            work_id, event.event_type
        );

        // Post work.started event
        self.client
            .post_work_started(
                &self.receiver_id,
                work_id,
                work_name,
                VERSION,
                &event.platform_id,
                &event.package,
                serde_json::json!({
                    "source_event_id": event.id,
                    "source_event_type": event.event_type
                }),
            )
            .await?;

        println!("Posted work.started for {}", work_id);

        // Simulate processing work
        // In a real application, this would be actual deployment logic
        let result = self.do_deployment_work(event).await;

        // Post work.completed or work.failed event based on result
        match result {
            Ok(details) => {
                self.client
                    .post_work_completed(
                        &self.receiver_id,
                        work_id,
                        work_name,
                        VERSION,
                        &event.platform_id,
                        &event.package,
                        true,
                        serde_json::json!({"result": details}),
                    )
                    .await?;
                println!("Posted work.completed for {}", work_id);
            }
            Err(err) => {
                self.client
                    .post_work_failed(
                        &self.receiver_id,
                        work_id,
                        work_name,
                        VERSION,
                        &event.platform_id,
                        &event.package,
                        &err.to_string(),
                        Some("DEPLOYMENT_ERROR"),
                    )
                    .await?;
                println!("Posted work.failed for {}: {}", work_id, err);
            }
        }

        Ok(())
    }

    /// Simulates deployment work.
    ///
    /// In a real application, this would be actual deployment logic.
    async fn do_deployment_work(
        &self,
        event: &CloudEventMessage,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        println!("Executing deployment for package: {}", event.package);

        // Simulate some work
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Check if the event indicates success
        if event.success {
            Ok(format!(
                "Successfully processed deployment for {}",
                event.package
            ))
        } else {
            Err("Source event indicates failure".into())
        }
    }
}

#[async_trait::async_trait]
impl MessageHandler for DeploymentHandler {
    async fn handle(
        &self,
        message: CloudEventMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Filter for deployment events
        if message.event_type.starts_with("deployment.") {
            self.process_deployment(&message).await?;
        } else {
            println!(
                "Ignoring non-deployment event: {} (type: {})",
                message.id, message.event_type
            );
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("xzatoma=debug".parse().unwrap()),
        )
        .init();

    println!("Starting {} downstream consumer...", SERVICE_NAME);

    // Load Kafka consumer configuration from environment
    let kafka_config = match KafkaConsumerConfig::from_env(SERVICE_NAME) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load Kafka config: {}", e);
            eprintln!("Please set the required environment variables:");
            eprintln!("  XZEPR_KAFKA_BROKERS");
            eprintln!("  XZEPR_KAFKA_TOPIC");
            eprintln!("  XZEPR_KAFKA_SECURITY_PROTOCOL (for SASL: SASL_SSL)");
            eprintln!("  XZEPR_KAFKA_SASL_USERNAME (if using SASL)");
            eprintln!("  XZEPR_KAFKA_SASL_PASSWORD (if using SASL)");
            return Err(e.into());
        }
    };

    println!("Kafka configuration:");
    println!("  Brokers: {}", kafka_config.brokers);
    println!("  Topic: {}", kafka_config.topic);
    println!("  Group ID: {}", kafka_config.group_id);
    println!(
        "  Security Protocol: {}",
        kafka_config.security_protocol.as_str()
    );

    // Create Kafka consumer
    let consumer = XzeprConsumer::new(kafka_config)?;

    // Create XZepr API client
    let api_client = match XzeprClient::from_env() {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to create XZepr client: {}", e);
            eprintln!("Please set the required environment variables:");
            eprintln!("  XZEPR_API_URL");
            eprintln!("  XZEPR_API_TOKEN");
            return Err(e.into());
        }
    };

    // Create the message handler
    let handler = Arc::new(DeploymentHandler::new(api_client).await?);

    println!("Consumer ready. Waiting for messages...");
    println!("Press Ctrl+C to stop.");

    // Run the consumer
    // Note: This is currently a stub implementation
    // With rdkafka feature enabled, this would actually consume from Kafka
    consumer.run(handler).await?;

    println!("Consumer stopped.");
    Ok(())
}
