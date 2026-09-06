import type { Order } from "@/lib/domain";

/**
 * Order predicates, mirroring `is_open` in `crates/ember-core/src/reducer.rs`.
 *
 * These are allow-lists on purpose, with an exhaustiveness check. The code this
 * replaced tested `order.status === "sent"` in six places to decide whether an
 * order could still be edited — a deny-list that was only correct while
 * `OrderStatus` had exactly two values. Adding `"completed"` would have made
 * every one of those checks silently wrong, letting the UI re-open a ticket the
 * kitchen had already sent out. The server would still have refused, but the
 * buttons would have looked live.
 */

function unreachableStatus(status: never): never {
  throw new Error(`Unhandled order status: ${String(status)}`);
}

/** True only while the order still accepts edits. */
export function isOpenOrder(order: Order | undefined): boolean {
  if (!order) return false;
  switch (order.status) {
    case "draft":
      return true;
    case "sent":
    case "completed":
      return false;
    default:
      return unreachableStatus(order.status);
  }
}

/**
 * True when an order exists but is closed to edits.
 *
 * Distinct from `!isOpenOrder`: a guest with no order yet has nothing to lock,
 * so controls stay enabled rather than being greyed out for the wrong reason.
 */
export function isLockedOrder(order: Order | undefined): boolean {
  return order !== undefined && !isOpenOrder(order);
}

/** Label for the send button / order state. */
export function orderStageLabel(order: Order | undefined): string {
  if (!order) return "Send order";
  switch (order.status) {
    case "draft":
      return "Send order";
    case "sent":
      return "Sent to kitchen";
    case "completed":
      return "Away from the pass";
    default:
      return unreachableStatus(order.status);
  }
}
