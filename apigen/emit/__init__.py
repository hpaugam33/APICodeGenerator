from .quarkus import emit_quarkus
from .rust import emit_rust

EMITTERS = {"quarkus": emit_quarkus, "rust": emit_rust}
