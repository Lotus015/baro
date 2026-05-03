/**
 * Critic — live acceptance-criteria evaluator.
 *
 * Observes ClaudeResultItem events on the bus. For each watched agent that
 * completes a turn without error, the Critic calls the Anthropic API and asks
 * the model whether the output satisfies the agent's acceptance criteria.
 *
 * The verdict is *always* published as a CritiqueItem (audit trail). On
 * "fail", an AgentTargetedMessageItem is emitted back to the agent as its
 * next conversational turn — up to `maxEmissionsPerAgent` times, after which
 * corrective messages are suppressed but CritiqueItem-s keep accumulating.
 *
 * Library-grade: no imports from prd.ts, story-agent.ts, or conductor.ts.
 */

import { ContextItem, Participant } from "@mozaik-ai/core"
import Anthropic from "@anthropic-ai/sdk"

import {
    AgentTargetedMessageItem,
    ClaudeResultItem,
    CritiqueItem,
} from "../types.js"

const VERDICT_SYSTEM_PROMPT = `\
You are a strict acceptance-criteria evaluator. You will receive:
1. A list of acceptance criteria that must ALL be satisfied.
2. The output text produced by an agent.

Evaluate whether every criterion is fully satisfied by the output.
Respond ONLY with a JSON object — no prose, no markdown fences — in exactly this shape:
{"verdict":"pass","reasoning":"…","violated_criteria":[]}
or
{"verdict":"fail","reasoning":"…","violated_criteria":["criterion A","criterion B"]}

Rules:
- "verdict" must be "pass" or "fail".
- "reasoning" must be a concise explanation (≤ 200 words).
- "violated_criteria" must list the exact criterion strings that are NOT satisfied.
- If ALL criteria pass, "violated_criteria" must be an empty array.
- Do NOT include any text outside the JSON object.`

export interface CriticOptions {
    /** Map from agentId to its acceptance-criteria strings. */
    targets: ReadonlyMap<string, readonly string[]>
    /** Max corrective AgentTargetedMessageItem-s per agent. Default: 2. */
    maxEmissionsPerAgent?: number
    /** Anthropic model used for verdict calls. Default: "claude-haiku-4-5". */
    model?: string
    /** Anthropic API key. Default: process.env.ANTHROPIC_API_KEY. */
    apiKey?: string
}

export class Critic extends Participant {
    private readonly opts: Required<CriticOptions>
    private readonly client: Anthropic
    /** agentId → number of AgentTargetedMessageItem-s emitted so far. */
    private readonly emissions = new Map<string, number>()
    /** agentId → number of result turns seen (for CritiqueItem.turn). */
    private readonly turnCount = new Map<string, number>()

    constructor(opts: CriticOptions) {
        super()
        this.opts = {
            maxEmissionsPerAgent: opts.maxEmissionsPerAgent ?? 2,
            model: opts.model ?? "claude-haiku-4-5",
            apiKey: opts.apiKey ?? process.env.ANTHROPIC_API_KEY ?? "",
            targets: opts.targets,
        }
        this.client = new Anthropic({ apiKey: this.opts.apiKey })
    }

    async onContextItem(source: Participant, item: ContextItem): Promise<void> {
        if (!(item instanceof ClaudeResultItem)) return
        if (item.isError || !item.resultText) return

        const criteria = this.opts.targets.get(item.agentId)
        if (!criteria || criteria.length === 0) return

        const turn = (this.turnCount.get(item.agentId) ?? 0) + 1
        this.turnCount.set(item.agentId, turn)

        const { verdict, reasoning, violatedCriteria } = await this.evaluate(
            item.resultText,
            criteria,
        )

        // Always emit audit trail.
        const critiqueItem = new CritiqueItem(
            item.agentId,
            verdict,
            reasoning,
            violatedCriteria,
            turn,
            this.opts.model,
        )
        for (const env of this.getEnvironments()) {
            env.deliverContextItem(this, critiqueItem)
        }

        // Emit corrective message only on fail and under the per-agent cap.
        if (verdict === "fail") {
            const emitted = this.emissions.get(item.agentId) ?? 0
            if (emitted < this.opts.maxEmissionsPerAgent) {
                this.emissions.set(item.agentId, emitted + 1)
                const text = buildCorrectiveMessage(reasoning, violatedCriteria)
                const msg = new AgentTargetedMessageItem(
                    item.agentId,
                    text,
                    { criticTurn: turn, emissionIndex: emitted + 1 },
                )
                for (const env of this.getEnvironments()) {
                    env.deliverContextItem(this, msg)
                }
            }
        }
    }

    private async evaluate(
        resultText: string,
        criteria: readonly string[],
    ): Promise<{
        verdict: "pass" | "fail"
        reasoning: string
        violatedCriteria: string[]
    }> {
        try {
            const response = await this.client.messages.create({
                model: this.opts.model,
                max_tokens: 1024,
                system: VERDICT_SYSTEM_PROMPT,
                messages: [
                    {
                        role: "user",
                        content: buildEvalPrompt(criteria, resultText),
                    },
                ],
            })

            const text = response.content
                .filter((b): b is Anthropic.TextBlock => b.type === "text")
                .map((b) => b.text)
                .join("")

            const parsed = JSON.parse(text) as {
                verdict: "pass" | "fail"
                reasoning: string
                violated_criteria: string[]
            }

            return {
                verdict: parsed.verdict === "pass" ? "pass" : "fail",
                reasoning: parsed.reasoning ?? "",
                violatedCriteria: Array.isArray(parsed.violated_criteria)
                    ? parsed.violated_criteria
                    : [],
            }
        } catch (err) {
            return {
                verdict: "fail",
                reasoning: `Critic LLM call failed: ${String(err)}`,
                violatedCriteria: ["[critic error — could not evaluate]"],
            }
        }
    }
}

function buildEvalPrompt(
    criteria: readonly string[],
    resultText: string,
): string {
    const criteriaList = criteria
        .map((c, i) => `${i + 1}. ${c}`)
        .join("\n")
    return [
        "## Acceptance criteria",
        criteriaList,
        "",
        "## Agent output",
        resultText,
    ].join("\n")
}

function buildCorrectiveMessage(
    reasoning: string,
    violatedCriteria: string[],
): string {
    const lines: string[] = [
        "Your output did not satisfy all acceptance criteria. Please revise.",
        "",
        `**Reasoning:** ${reasoning}`,
    ]
    if (violatedCriteria.length > 0) {
        lines.push("", "**Violated criteria:**")
        for (const c of violatedCriteria) {
            lines.push(`- ${c}`)
        }
    }
    lines.push("", "Please address the above and resubmit your work.")
    return lines.join("\n")
}
