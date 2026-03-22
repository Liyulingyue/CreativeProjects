import asyncio
from openai import AsyncOpenAI
from ragas.llms import llm_factory
from ragas.metrics.collections import ContextPrecision

async def main():
    # Setup LLM - Notice using AsyncOpenAI for async metrics
    client = AsyncOpenAI(
        api_key="**********",
        base_url="https://aistudio.baidu.com/llm/lmapi/v3"
    )
    llm = llm_factory(model="ernie-4.5-21b-a3b", client=client)

    # Create metric
    scorer = ContextPrecision(llm=llm)

    # Evaluate
    # Note: Ragas metrics ascore returns the score directly or an object based on version
    result = await scorer.ascore(
        user_input="Where is the Eiffel Tower located?",
        reference="The Eiffel Tower is located in Paris.",
        retrieved_contexts=[
            "The Eiffel Tower is located in Paris.",
            "The Brandenburg Gate is located in Berlin."
        ]
    )
    print(f"Context Precision Score: {result}")

if __name__ == "__main__":
    asyncio.run(main())
