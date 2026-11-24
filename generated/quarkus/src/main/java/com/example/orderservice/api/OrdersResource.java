// GENERATED CODE — DO NOT EDIT.
// Source of truth: OpenAPI/AsyncAPI specs. Regenerate with:
//   apigen --openapi specs/order-service.openapi.yaml --asyncapi specs/order-events.asyncapi.yaml --target both --out generated --package com.example.orderservice --crate order_service

package com.example.orderservice.api;

import com.example.orderservice.model.CreateOrderRequest;
import com.example.orderservice.model.Order;
import com.example.orderservice.model.OrderStatus;
import com.example.orderservice.model.UpdateOrderStatusRequest;
import jakarta.ws.rs.Consumes;
import jakarta.ws.rs.DELETE;
import jakarta.ws.rs.GET;
import jakarta.ws.rs.PATCH;
import jakarta.ws.rs.POST;
import jakarta.ws.rs.Path;
import jakarta.ws.rs.PathParam;
import jakarta.ws.rs.Produces;
import jakarta.ws.rs.QueryParam;
import jakarta.ws.rs.core.MediaType;
import jakarta.ws.rs.core.Response;
import java.util.List;
import java.util.UUID;

@Path("/orders")
@Produces(MediaType.APPLICATION_JSON)
@Consumes(MediaType.APPLICATION_JSON)
public class OrdersResource {

    @GET
    /** List orders, optionally filtered by status (paginated). */
    public List<Order> listOrders(@QueryParam("limit") Integer limit, @QueryParam("offset") Integer offset, @QueryParam("status") OrderStatus status) {
        throw new UnsupportedOperationException("TODO implement listOrders");
    }

    @POST
    /** Create an order for a customer. */
    public Order createOrder(CreateOrderRequest body) {
        throw new UnsupportedOperationException("TODO implement createOrder");
    }

    @DELETE
    @Path("/{orderId}")
    /** Delete (cancel) an order. */
    public Response deleteOrder(@PathParam("orderId") UUID orderId) {
        throw new UnsupportedOperationException("TODO implement deleteOrder");
    }

    @GET
    @Path("/{orderId}")
    /** Get an order by id. */
    public Order getOrder(@PathParam("orderId") UUID orderId) {
        throw new UnsupportedOperationException("TODO implement getOrder");
    }

    @PATCH
    @Path("/{orderId}/status")
    /** Change the status of an order. */
    public Order updateOrderStatus(@PathParam("orderId") UUID orderId, UpdateOrderStatusRequest body) {
        throw new UnsupportedOperationException("TODO implement updateOrderStatus");
    }
}
