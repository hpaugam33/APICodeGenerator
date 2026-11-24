// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   python -m apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.api;

import com.example.orderservice.model.CreateCustomerRequest;
import com.example.orderservice.model.Customer;
import jakarta.ws.rs.Consumes;
import jakarta.ws.rs.GET;
import jakarta.ws.rs.POST;
import jakarta.ws.rs.Path;
import jakarta.ws.rs.PathParam;
import jakarta.ws.rs.Produces;
import jakarta.ws.rs.core.MediaType;
import java.util.UUID;

@Path("/customers")
@Produces(MediaType.APPLICATION_JSON)
@Consumes(MediaType.APPLICATION_JSON)
public class CustomersResource {

    @POST
    /** Register a new customer. */
    public Customer createCustomer(CreateCustomerRequest body) {
        throw new UnsupportedOperationException("TODO implement createCustomer");
    }

    @GET
    @Path("/{customerId}")
    /** Get a customer by id. */
    public Customer getCustomer(@PathParam("customerId") UUID customerId) {
        throw new UnsupportedOperationException("TODO implement getCustomer");
    }
}
