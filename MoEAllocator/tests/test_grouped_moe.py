import unittest

import torch
import torch.nn.functional as F

from src.moe_grouped import grouped_moe_forward


class GroupedMoeForwardTest(unittest.TestCase):
    def test_grouped_forward_matches_reference_expert_loop(self):
        torch.manual_seed(7)
        num_experts = 4
        num_tokens = 5
        num_top_k = 3
        hidden_size = 16
        intermediate_size = 8

        hidden_states = torch.randn(num_tokens, hidden_size, dtype=torch.float16)
        gate_up_proj = torch.randn(num_experts, 2 * intermediate_size, hidden_size, dtype=torch.float16)
        down_proj = torch.randn(num_experts, hidden_size, intermediate_size, dtype=torch.float16)
        top_k_index = torch.tensor(
            [
                [2, 1, 3],
                [0, 3, 2],
                [3, 2, 1],
                [1, 0, 3],
                [2, 3, 0],
            ]
        )
        top_k_weights = torch.softmax(torch.randn(num_tokens, num_top_k), dim=-1).to(torch.float16)

        actual = grouped_moe_forward(
            hidden_states,
            top_k_index,
            top_k_weights,
            gate_up_proj,
            down_proj,
            num_experts,
        )

        expected_pairs = []
        for token_idx in range(num_tokens):
            for top_k_pos in range(num_top_k):
                expert_id = int(top_k_index[token_idx, top_k_pos])
                gate, up = F.linear(hidden_states[token_idx : token_idx + 1], gate_up_proj[expert_id]).chunk(2, dim=-1)
                output = F.linear(F.silu(gate) * up, down_proj[expert_id])
                expected_pairs.append(output[0] * top_k_weights[token_idx, top_k_pos])
        expected = torch.stack(expected_pairs).view(num_tokens, num_top_k, hidden_size).sum(dim=1)

        torch.testing.assert_close(actual, expected)


if __name__ == "__main__":
    unittest.main()
