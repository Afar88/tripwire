/**
 * OutLayer approver adapter (STUB / DRY RUN).
 *
 * Idea: OutLayer agent wallets can require multisig approval. Add a Tripwire-run NEAR
 * account as one approver in the owner's OutLayer policy. For every pending operation,
 * Tripwire checks the payee and:
 *   ALLOW -> votes approve   (low risk goes through without bothering the owner)
 *   BLOCK -> votes reject    (high risk never goes out)
 *   WARN  -> does not vote   (left for the human approver)
 * The agent cannot bypass this: the vote is part of OutLayer's TEE-enforced policy, not
 * something the agent's prompt controls.
 *
 * What is REAL here: the decision logic (`decide`, `decideApproval`), driven by the live
 * Tripwire registry on testnet, and its unit tests.
 * What is NOT done yet: reading real pending approvals from OutLayer and submitting a
 * signed vote. `PendingApproval` is our assumed shape (based on OutLayer's public docs:
 * GET /wallet/v1/approval/{id} returns op_canonical + request_hash; approvers sign a
 * NEP-413 message `approve:<approval_id>:<wallet_pubkey>:<request_hash>`), not verified
 * against a live OutLayer testnet wallet. `submitVote` throws on purpose.
 */
import { checkTransaction, type RegistrySource, type TxLike, type Verdict } from "./tripwire.js";

export type Vote = "approve" | "reject" | "leave_for_owner";

/** Assumed, simplified view of one OutLayer pending approval (not verified against the live API). */
export interface PendingApproval {
  approval_id: string;
  wallet_pubkey: string;
  request_hash: string;
  op: {
    type: "transfer" | "call" | "ft_transfer";
    /** Payee for transfers, contract for calls. */
    receiver_id: string;
    amount?: string;
    method?: string;
    args?: Record<string, unknown>;
  };
}

export const OUTLAYER_TESTNET_API = "https://testnet-api.outlayer.ai";

export function decide(verdict: Verdict): Vote {
  return verdict === "ALLOW" ? "approve" : verdict === "BLOCK" ? "reject" : "leave_for_owner";
}

/** Turn an OutLayer op into the transaction shape the checker understands. */
export function opToTx(op: PendingApproval["op"]): TxLike {
  return {
    receiverId: op.receiver_id,
    actions: [{ type: op.type === "transfer" ? "Transfer" : "FunctionCall", params: { methodName: op.method, args: op.args } }],
  };
}

/** The NEP-413 message an approver would sign (format from OutLayer docs; reject format assumed symmetric). */
export function voteMessage(vote: Exclude<Vote, "leave_for_owner">, a: PendingApproval): string {
  return `${vote}:${a.approval_id}:${a.wallet_pubkey}:${a.request_hash}`;
}

export async function decideApproval(a: PendingApproval, registry: RegistrySource) {
  const { verdict, results } = await checkTransaction(opToTx(a.op), registry);
  const vote = decide(verdict);
  return { approval_id: a.approval_id, verdict, vote, message: vote === "leave_for_owner" ? null : voteMessage(vote, a), results };
}

/** NOT IMPLEMENTED: needs a Tripwire approver account registered in a real OutLayer testnet policy. */
export async function submitVote(_api: string, _a: PendingApproval, _vote: Vote): Promise<never> {
  throw new Error("submitVote is a stub: OutLayer vote submission is not wired up yet (dry run only)");
}
