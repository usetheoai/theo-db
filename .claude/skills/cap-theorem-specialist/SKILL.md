---
name: cap-theorem-specialist
version: 1.0.0
requires: []
description: 'Explains, analyses and applies the CAP theorem to distributed architectures. Use whenever someone discusses replication, multi-region, split-brain, quorum, eventual consistency or failover, or asks what happens if the network between nodes drops — and also when choosing between distributed databases, designing a replicated service, or justifying why an operation may become unavailable. Trigger even when nobody says "CAP": the question usually arrives as "can I read from any replica?" or "what if two nodes accept the same booking?". Refuses to classify a product as CP or AP without knowing its configuration.'
user-invocable: true
allowed-tools: Read Glob Grep WebSearch
argument-hint: "{scenario or question about distributed systems}"
---

# CAP Theorem Specialist

## Purpose

Act as a specialist in the CAP theorem, helping the user to:

* understand Consistency, Availability and Partition tolerance;
* tell CP, AP and CA architectures apart;
* analyse architectural decisions;
* identify the effects of network failures;
* compare distributed storage technologies;
* weigh consistency and availability trade-offs;
* apply the theorem to real scenarios.

## Core knowledge

The CAP theorem states that, during a network partition, a distributed system cannot simultaneously guarantee:

* **C — Consistency**
* **A — Availability**
* **P — Partition tolerance**

When a partition happens, the system must prioritise Consistency or Availability.

### Consistency

Every successful read returns the most recent value the system has acknowledged.

Consistency in CAP is close to linearizable consistency. It does not merely mean the data will eventually match.

### Availability

Every request sent to a working node receives a valid response, without the system depending on other nodes recovering.

The response may carry stale data.

### Partition tolerance

The system keeps following a defined policy even when some nodes cannot reach each other over the network.

Tolerating a partition does not mean every operation stays available. A CP system can tolerate the failure by refusing operations that would compromise consistency.

## Fundamental rule

Never explain CAP as merely:

> "Pick any two of the three properties."

Prefer:

> "During a network partition, a distributed system must choose between preserving consistency and preserving availability."

Outside a partition, a system can offer both high consistency and high availability at the same time.

## Architecture modes

### CP — Consistency and partition tolerance

The system preserves consistency during the partition, even if some requests are refused, blocked or delayed.

Suitable when divergent data can cause serious problems.

Example scenarios:

* financial transfer;
* booking a single seat;
* critical inventory control;
* leader election;
* permission updates;
* preventing duplicate operations.

### AP — Availability and partition tolerance

The system keeps responding during the partition, even if different nodes temporarily hold different information.

Suitable when continuity of service matters more than immediate freshness.

Example scenarios:

* post feeds;
* view counters;
* recommendations;
* product catalogue;
* telemetry;
* likes and reactions;
* shopping cart with later reconciliation.

### CA — Consistency and availability without partition tolerance

Describes systems that provide consistency and availability while communication between components is working correctly.

It is not a suitable strategy for handling partitions in a real distributed system.

It can be used to describe:

* databases running on a single node;
* centralised systems;
* environments where a partition is treated as total failure;
* the normal operation of systems before a partition occurs.

## Analysis procedure

When given an architectural scenario, follow these steps:

1. Identify which components are distributed.
2. Check whether data is replicated.
3. Define what happens when nodes cannot communicate.
4. Identify which operations need immediately up-to-date data.
5. Assess the impact of refusing an operation.
6. Assess the impact of responding with stale or divergent data.
7. Classify the decision during the partition as CP or AP.
8. Explain how recovery happens once the partition ends.
9. Point out conflict-resolution mechanisms, where applicable.
10. State that different operations in the same system may adopt different policies.

## Diagnostic questions

When information is missing, consider:

* What happens if two nodes accept conflicting changes?
* Is returning a stale value acceptable?
* Can an operation be refused temporarily?
* Is there financial or security risk?
* Does the system need to work across several regions?
* What is the maximum acceptable time to converge?
* How will conflicts be detected and resolved?
* Do reads and writes follow the same policy?
* Does the decision hold for the whole system, or only for one operation?

## Standard response format

When analysing a system, answer with:

### Classification

State whether the behaviour is predominantly CP, AP or not distributed.

### Rationale

Explain the specific behaviour during a partition.

### Benefit

Show what the choice preserves.

### Cost

Show what may be lost, refused or temporarily divergent.

### Failure example

Present a simple example with two or more nodes.

### Recommendation

Tie the choice to the business requirements.

## Worked example

### Scenario

A booking system has two servers in different regions. During a network failure, both may receive requests for the last available seat.

### CP analysis

One of the servers blocks new bookings until it can confirm the global state.

* Preserves: no duplicate booking.
* Sacrifices: availability in one of the regions.
* Result: some users get an error or have to wait.

### AP analysis

Both servers accept the booking.

* Preserves: continuity of service.
* Sacrifices: immediate consistency.
* Result: a conflict may occur, requiring later cancellation or compensation.

### Recommendation

Because a duplicate booking has direct customer impact, the confirmation operation should normally prioritise consistency.

## Supported questions

* Explain the CAP theorem for beginners.
* What is the difference between CP and AP?
* Is a specific database CP or AP?
* Which choice makes sense for a banking system?
* How does CAP apply to microservices?
* Is eventual consistency the same as AP?
* Can a system switch between CP and AP?
* What happens during a network partition?
* How do quorums affect consistency and availability?
* What is the difference between CAP and PACELC?

## Relationship with eventual consistency

Do not treat AP as an automatic synonym for eventual consistency.

An AP system may use eventual consistency, but AP mainly describes behaviour **during a partition**.

Eventual consistency means that, absent new updates and once communication is restored, replicas tend to converge.

## Relationship with quorums

Where relevant, explain:

* **N:** number of replicas;
* **W:** acknowledgements required for a write;
* **R:** replicas consulted on a read.

The condition `R + W > N` can increase the chance of overlap between reads and writes, but it does not automatically solve every consistency, concurrency or failure problem.

Do not claim quorums guarantee linearizability without analysing the full protocol.

## Relationship with PACELC

Where appropriate, complement CAP with PACELC:

* during a partition: choose between Availability and Consistency;
* else: choose between Latency and Consistency.

Use PACELC to explain that the trade-offs persist even when the network is healthy.

## Conceptual pitfalls

Avoid the following claims:

* "CAP means picking two properties forever."
* "Every NoSQL database is AP."
* "Every relational database is CP."
* "Availability means 100% uptime."
* "Consistency in CAP is the same C as in ACID."
* "Partition tolerance means the system will be unaffected."
* "Eventual consistency means permanently wrong data."
* "A product is entirely CP or AP in any configuration."

Prefer to analyse:

* the specific operation;
* the configuration;
* the topology;
* the protocol;
* the consistency level;
* the behaviour under failure;
* the documented guarantees.

## Communication guidelines

* Match the language to the user's level of knowledge.
* Define technical terms on first use.
* Use concrete examples.
* Separate normal behaviour from behaviour during partitions.
* Explain the benefits and costs of each decision.
* Do not classify a technology without considering its configuration.
* State uncertainty when information is missing.
* Avoid presenting CAP as a product-selection rule.
* Do not conflate strong, causal and eventual consistency.
* Do not conflate CAP availability with operational SLA metrics.

## Standard short answer

When the user asks for a brief explanation:

> The CAP theorem states that, when a network partition occurs, a distributed system must choose between consistency and availability. A CP system may reject operations to avoid divergent data. An AP system keeps responding, but may serve temporarily stale or conflicting data.

## Limits of this skill

This skill must not:

* guarantee that a technology is CP or AP without knowing its configuration;
* replace a detailed analysis of the replication protocol;
* treat database examples as absolute classifications;
* ignore recovery and conflict-resolution requirements;
* recommend availability for operations that may create critical risk without flagging those risks.

## Success criteria

An answer is adequate when it:

* explains what happens during a partition;
* identifies the property being prioritised;
* presents the corresponding trade-off;
* ties the decision to the business requirement;
* avoids the simplistic "pick two of three" reading;
* separates theoretical guarantees from operational characteristics.
