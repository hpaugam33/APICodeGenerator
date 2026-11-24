# apigen — OpenAPI/AsyncAPI → service API interfaces + data model code

`apigen` turns API spec files into service code:

```
API specs                                 apigen                       your services
┌──────────────────────┐                  ┌────────────────────┐   gen   ┌──────────────────────┐
│ OpenAPI 3.x spec     │ ───────────────► │                    │ ──────► │ REST API interface   │
│ AsyncAPI 2.x/3.x     │                  │      apigen        │         │ message producers/   │
└──────────────────────┘                  └────────────────────┘         │ consumers + data model│
                                                                      └──────────────────────┘
                                                                             Java Quarkus | Rust
```

From the spec files it **fully regenerates** (spec = source of truth) the API
interface of a service — REST resources/handlers, message producers/consumers —
and the **data model** classes, in **Java Quarkus** and/or **Rust**.

## Quickstart

```bash
cargo install --path .        # or: cargo build --release -> target/release/apigen

apigen \
  --openapi  specs/order-service.openapi.yaml \
  --asyncapi specs/order-events.asyncapi.yaml \
  --target both \
  --out generated \
  --package com.example.orderservice \
  --crate order_service

cargo test                    # test suite
```

`specs/` contains a sample spec set (Order/Customer model + order/payment event
flows) used as the demo input. Replace with your own specs.

### CLI options

| option | meaning | default |
|---|---|---|
| `--openapi FILE` | OpenAPI 3.x spec (repeatable) | — |
| `--asyncapi FILE` | AsyncAPI 2.x/3.x spec (repeatable) | — |
| `--target` | `quarkus`, `rust` or `both` | `both` |
| `--out DIR` | output root; each target dir is wiped and rebuilt | `generated` |
| `--package` | Java base package | `com.example.<api>` |
| `--group-id` / `--artifact` | Maven coordinates | `com.example` / api name |
| `--crate` | Rust crate name | api name in snake_case |
| `--api-name` | override API name | `info.title` of first spec |

## What gets generated

### Quarkus (`generated/quarkus/`)

```
pom.xml                                       Quarkus 3.15 BOM, Java 21
src/main/resources/application.properties     mp.messaging.* channel/topic wiring from AsyncAPI bindings
src/main/java/<pkg>/model/*.java              records + enums with Bean Validation (@NotNull, @Size, @Email, ...)
src/main/java/<pkg>/api/*Resource.java        one JAX-RS resource per OpenAPI tag, TODO stubs
src/main/java/<pkg>/messaging/*.java          @Channel Emitter producers / @Incoming consumers per channel
```

### Rust (`generated/rust/`)

```
Cargo.toml            axum 0.8, tokio, serde, async-trait, anyhow, chrono, uuid
src/main.rs           router wiring, binds 127.0.0.1:8080
src/api/<tag>.rs      axum handlers + router per tag, path/query structs, ApiError, todo!() bodies
src/model/*.rs        serde structs (camelCase) / enums (SCREAMING_SNAKE_CASE)
src/messaging/mod.rs  #[async_trait] producer/consumer traits per channel — broker-agnostic
```

Verified in this sandbox: the Rust crate passes `cargo check`; the Java tree
compiles with `javac` against the Jakarta/MicroProfile API jars.

## Update semantics — full regenerate

Each run deletes and rebuilds the target directory. The specs are the single
source of truth, so adding/removing/renaming a model class or event flow in
the specs is enough to update every service interface.
Hand-written code must live **outside** the generated packages
(e.g. `service/`, `infra/` in Quarkus; your own modules next to `api/`,
`model/`, `messaging/` in Rust, plus broker implementations of the generated
messaging traits).

## Choosing Quarkus vs Rust for high exchange volume

Both generated skeletons are reactive/non-blocking end to end. For a service
that must handle a lot of exchanges (many information flows, high event
throughput):

- **Rust** — lowest per-message cost and memory footprint; the generated
  `async_trait` producer/consumer traits map directly onto `rdkafka`/`lapin`
  with zero-cost dispatch; backpressure comes from your broker client. Best
  when throughput and resource efficiency dominate.
- **Java Quarkus** — smallest team ramp-up, rich ecosystem (SmallRye Reactive
  Messaging gives you Kafka wiring, retries, dead-letter topics declaratively
  via `application.properties`, already generated); with virtual threads /
  reactive messaging it scales well, at somewhat higher memory per instance.

Rule of thumb: many services, moderate flows, Java team → **Quarkus**; few
hot services, very high message rates, cost/latency-critical → **Rust**.
The generator emits both from the same IR, so you can benchmark both with the
same specs.

## How the specs are consumed

- OpenAPI `components.schemas` → data model classes (objects → records/structs,
  `enum` → enums, `allOf` → flattened fields, inline object/enum properties →
  named sub-models `<Parent><Property>`).
- `paths` + `operationId` + first `tag` → REST interface; path/query/header
  params and JSON request/response bodies are typed; the first 2xx response is
  the return type, first 4xx/5xx is kept as the error model.
- AsyncAPI 3.0 `operations` (or 2.x `publish`/`subscribe`) → send/receive
  flows per channel; `bindings.kafka.topic` (or the channel address) becomes
  the topic; message `payload` → typed event model.
- Cross-file `$ref`s (`./order-service.openapi.yaml#/components/schemas/Order`)
  are resolved, so the shared data model can live in the OpenAPI spec and be
  referenced from the AsyncAPI spec.
- Names with spaces/accents ("Détails de la commande") are normalized to
  language-safe identifiers (`DetailsDeLaCommande` / `details_de_la_commande`).

## Limitations / next steps

- One JSON content type per operation; `allOf` inheritance is flattened, not
  mapped to class hierarchies; no polymorphism (`oneOf`/`anyOf` → best-effort).
- Validation constraints are emitted for Java (Bean Validation); Rust models
  rely on serde types only (add `validator` if you need the same constraints).
- Messaging config assumes Kafka bindings (topic from `bindings.kafka.topic`);
  other brokers need their binding read — the Rust traits stay broker-agnostic.
- Handlers are stubs (`UnsupportedOperationException` / `todo!()`) by design:
  the generator owns the interface, you own the behavior.
