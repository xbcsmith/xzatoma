# Kafka Transient Consumer Resilience Implementation

## Overview

The XZepr watcher now treats short-lived Kafka transport failures as retryable
receive errors instead of shutting down the watcher immediately.

This fixes startup behavior where `rdkafka` can report a failed bootstrap
connection, such as an IPv6 `localhost` refusal, while it is still able to keep
trying other broker addresses.

## Behavior

The XZepr consumer logs and continues when Kafka receive errors include:

- `BrokerTransportFailure`
- `AllBrokersDown`
- `NetworkException`

Other Kafka receive errors still stop the watcher and return a
`ConsumerError::Kafka`, preserving the existing fatal-error behavior for
non-retryable conditions.

## Scope

The change is limited to the XZepr consumer loop. Message decoding, CloudEvents
parsing, handler dispatch, topic administration, and generic watcher behavior
remain unchanged.

## Verification

Unit tests cover the transient-error classifier so broker transport failures
match the retry path and unrelated Kafka errors continue to be treated as fatal.
