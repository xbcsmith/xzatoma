//! Integration tests for conversation persistence and replay functionality
//!
//! Tests the complete workflow of persisting subagent conversations,
//! listing them, and replaying them.

use tempfile::TempDir;
use xzatoma::agent::{
    new_conversation_id, now_rfc3339, ConversationMetadata, ConversationRecord, ConversationStore,
};
use xzatoma::providers::Message;

#[test]
fn test_persistence_create_and_retrieve_single_conversation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let store = ConversationStore::new(&db_path).expect("Failed to create store");

    let record = ConversationRecord {
        id: new_conversation_id(),
        parent_id: None,
        label: "test_task".to_string(),
        depth: 1,
        messages: vec![
            Message::user("Task: analyze this code"),
            Message::assistant("I'll analyze the code..."),
        ],
        started_at: now_rfc3339(),
        completed_at: None,
        metadata: ConversationMetadata {
            turns_used: 2,
            tokens_consumed: 150,
            completion_status: "incomplete".to_string(),
            max_turns_reached: false,
            task_prompt: "analyze this code".to_string(),
            summary_prompt: None,
            allowed_tools: vec!["file_ops".to_string()],
        },
    };

    let id = record.id.clone();
    store.save(&record).expect("Failed to save record");

    let retrieved = store.get(&id).expect("Failed to get record");
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, id);
    assert_eq!(retrieved.label, "test_task");
    assert_eq!(retrieved.depth, 1);
    assert_eq!(retrieved.metadata.turns_used, 2);
    assert_eq!(retrieved.messages.len(), 2);
}

#[test]
fn test_persistence_list_multiple_conversations() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let store = ConversationStore::new(&db_path).expect("Failed to create store");

    // Create 3 conversations
    for i in 0..3 {
        let record = ConversationRecord {
            id: new_conversation_id(),
            parent_id: None,
            label: format!("task_{}", i),
            depth: 1,
            messages: vec![],
            started_at: now_rfc3339(),
            completed_at: None,
            metadata: ConversationMetadata::default(),
        };
        store.save(&record).expect("Failed to save record");
    }

    let all_records = store.list(10, 0).expect("Failed to list records");
    assert_eq!(all_records.len(), 3);

    for record in all_records.iter() {
        assert!(record.label.starts_with("task_"));
        assert_eq!(record.depth, 1);
    }
}

#[test]
fn test_persistence_pagination() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let store = ConversationStore::new(&db_path).expect("Failed to create store");

    // Create 5 conversations
    for i in 0..5 {
        let record = ConversationRecord {
            id: new_conversation_id(),
            parent_id: None,
            label: format!("task_{}", i),
            depth: 0,
            messages: vec![],
            started_at: now_rfc3339(),
            completed_at: None,
            metadata: ConversationMetadata::default(),
        };
        store.save(&record).expect("Failed to save record");
    }

    // Test pagination
    let page1 = store.list(2, 0).expect("Failed to list page 1");
    assert_eq!(page1.len(), 2);

    let page2 = store.list(2, 2).expect("Failed to list page 2");
    assert_eq!(page2.len(), 2);

    let page3 = store.list(2, 4).expect("Failed to list page 3");
    assert_eq!(page3.len(), 1);

    let page4 = store.list(2, 6).expect("Failed to list page 4");
    assert_eq!(page4.len(), 0);
}

#[test]
fn test_persistence_parent_child_relationships() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let store = ConversationStore::new(&db_path).expect("Failed to create store");

    // Create parent conversation
    let parent_id = new_conversation_id();
    let parent = ConversationRecord {
        id: parent_id.clone(),
        parent_id: None,
        label: "parent_task".to_string(),
        depth: 0,
        messages: vec![],
        started_at: now_rfc3339(),
        completed_at: None,
        metadata: ConversationMetadata::default(),
    };
    store.save(&parent).expect("Failed to save parent");

    // Create child conversations
    for i in 0..3 {
        let child = ConversationRecord {
            id: new_conversation_id(),
            parent_id: Some(parent_id.clone()),
            label: format!("subtask_{}", i),
            depth: 1,
            messages: vec![],
            started_at: now_rfc3339(),
            completed_at: None,
            metadata: ConversationMetadata::default(),
        };
        store.save(&child).expect("Failed to save child");
    }

    // Verify we can find children by parent
    let children = store
        .find_by_parent(&parent_id)
        .expect("Failed to find children");
    assert_eq!(children.len(), 3);

    for child in children {
        assert_eq!(child.parent_id, Some(parent_id.clone()));
        assert!(child.label.starts_with("subtask_"));
    }
}

#[test]
fn test_persistence_conversation_tree_depth() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let store = ConversationStore::new(&db_path).expect("Failed to create store");

    // Create a tree: root -> level1 -> level2
    let root_id = new_conversation_id();
    let root = ConversationRecord {
        id: root_id.clone(),
        parent_id: None,
        label: "root".to_string(),
        depth: 0,
        messages: vec![],
        started_at: now_rfc3339(),
        completed_at: None,
        metadata: ConversationMetadata::default(),
    };
    store.save(&root).expect("Failed to save root");

    let level1_id = new_conversation_id();
    let level1 = ConversationRecord {
        id: level1_id.clone(),
        parent_id: Some(root_id.clone()),
        label: "level1".to_string(),
        depth: 1,
        messages: vec![],
        started_at: now_rfc3339(),
        completed_at: None,
        metadata: ConversationMetadata::default(),
    };
    store.save(&level1).expect("Failed to save level1");

    let level2 = ConversationRecord {
        id: new_conversation_id(),
        parent_id: Some(level1_id.clone()),
        label: "level2".to_string(),
        depth: 2,
        messages: vec![],
        started_at: now_rfc3339(),
        completed_at: None,
        metadata: ConversationMetadata::default(),
    };
    store.save(&level2).expect("Failed to save level2");

    // Verify tree structure
    let root_retrieved = store.get(&root_id).expect("Failed to get root");
    assert!(root_retrieved.is_some());
    assert_eq!(root_retrieved.unwrap().depth, 0);

    let level1_retrieved = store.get(&level1_id).expect("Failed to get level1");
    assert!(level1_retrieved.is_some());
    assert_eq!(level1_retrieved.unwrap().depth, 1);

    let level1_children = store
        .find_by_parent(&level1_id)
        .expect("Failed to find level1 children");
    assert_eq!(level1_children.len(), 1);
    assert_eq!(level1_children[0].depth, 2);
}

#[test]
fn test_persistence_metadata_fields() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let store = ConversationStore::new(&db_path).expect("Failed to create store");

    let record = ConversationRecord {
        id: new_conversation_id(),
        parent_id: None,
        label: "complex_task".to_string(),
        depth: 1,
        messages: vec![],
        started_at: now_rfc3339(),
        completed_at: Some(now_rfc3339()),
        metadata: ConversationMetadata {
            turns_used: 5,
            tokens_consumed: 1000,
            completion_status: "complete".to_string(),
            max_turns_reached: true,
            task_prompt: "Do complex work".to_string(),
            summary_prompt: Some("Summarize results".to_string()),
            allowed_tools: vec!["file_ops".to_string(), "terminal".to_string()],
        },
    };

    let id = record.id.clone();
    store.save(&record).expect("Failed to save record");

    let retrieved = store.get(&id).expect("Failed to get record");
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.metadata.turns_used, 5);
    assert_eq!(retrieved.metadata.tokens_consumed, 1000);
    assert_eq!(retrieved.metadata.completion_status, "complete");
    assert!(retrieved.metadata.max_turns_reached);
    assert_eq!(
        retrieved.metadata.task_prompt,
        "Do complex work".to_string()
    );
    assert_eq!(
        retrieved.metadata.summary_prompt,
        Some("Summarize results".to_string())
    );
    assert_eq!(retrieved.metadata.allowed_tools.len(), 2);
}

#[test]
fn test_persistence_serialization_roundtrip() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let store = ConversationStore::new(&db_path).expect("Failed to create store");

    let original = ConversationRecord {
        id: new_conversation_id(),
        parent_id: Some(new_conversation_id()),
        label: "test".to_string(),
        depth: 2,
        messages: vec![
            Message::user("User message"),
            Message::assistant("Assistant response"),
        ],
        started_at: "2025-11-07T18:12:07+00:00".to_string(),
        completed_at: Some("2025-11-07T18:13:07+00:00".to_string()),
        metadata: ConversationMetadata {
            turns_used: 3,
            tokens_consumed: 500,
            completion_status: "complete".to_string(),
            max_turns_reached: false,
            task_prompt: "Task".to_string(),
            summary_prompt: Some("Summary".to_string()),
            allowed_tools: vec!["tool1".to_string()],
        },
    };

    let id = original.id.clone();
    store.save(&original).expect("Failed to save record");

    let retrieved = store.get(&id).expect("Failed to get record");
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, original.id);
    assert_eq!(retrieved.parent_id, original.parent_id);
    assert_eq!(retrieved.label, original.label);
    assert_eq!(retrieved.depth, original.depth);
    assert_eq!(retrieved.messages.len(), original.messages.len());
    assert_eq!(retrieved.started_at, original.started_at);
    assert_eq!(retrieved.completed_at, original.completed_at);
    assert_eq!(retrieved.metadata.turns_used, original.metadata.turns_used);
}

#[test]
fn test_persistence_multiple_databases_isolated() {
    let temp_dir1 = TempDir::new().expect("Failed to create temp dir");
    let temp_dir2 = TempDir::new().expect("Failed to create temp dir");
    let db_path1 = temp_dir1.path().join("test1.db");
    let db_path2 = temp_dir2.path().join("test2.db");

    let store1 = ConversationStore::new(&db_path1).expect("Failed to create store1");
    let store2 = ConversationStore::new(&db_path2).expect("Failed to create store2");

    // Save to store1
    let record1 = ConversationRecord {
        id: new_conversation_id(),
        parent_id: None,
        label: "store1_task".to_string(),
        depth: 0,
        messages: vec![],
        started_at: now_rfc3339(),
        completed_at: None,
        metadata: ConversationMetadata::default(),
    };
    let id1 = record1.id.clone();
    store1.save(&record1).expect("Failed to save to store1");

    // Save to store2
    let record2 = ConversationRecord {
        id: new_conversation_id(),
        parent_id: None,
        label: "store2_task".to_string(),
        depth: 0,
        messages: vec![],
        started_at: now_rfc3339(),
        completed_at: None,
        metadata: ConversationMetadata::default(),
    };
    let id2 = record2.id.clone();
    store2.save(&record2).expect("Failed to save to store2");

    // Verify isolation
    let from_store1 = store1.get(&id1).expect("Failed to get from store1");
    assert!(from_store1.is_some());
    assert_eq!(from_store1.unwrap().label, "store1_task");

    let from_store2 = store2.get(&id2).expect("Failed to get from store2");
    assert!(from_store2.is_some());
    assert_eq!(from_store2.unwrap().label, "store2_task");

    // Verify cross-database queries fail
    let cross_query = store1.get(&id2).expect("Failed to query store1");
    assert!(cross_query.is_none());
}
