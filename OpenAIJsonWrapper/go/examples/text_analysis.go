package main

import (
	"fmt"
	"log"
	"os"

	openaijsonwrapper "openaijsonwrapper/src"
)

func main() {
	apiKey := os.Getenv("OPENAI_API_KEY")
	if apiKey == "" {
		apiKey = "your-api-key"
		fmt.Println("警告: 未检测到 OPENAI_API_KEY 环境变量")
	}

	baseURL := os.Getenv("OPENAI_BASE_URL")
	if baseURL == "" {
		baseURL = "https://api.minimaxi.com/v1"
	}

	modelName := os.Getenv("OPENAI_MODEL_NAME")
	if modelName == "" {
		modelName = "MiniMax-M3"
	}

	targetStructure := map[string]any{
		"analysis": map[string]any{
			"sentiment":        "string (Positive/Negative/Neutral)",
			"key_entities":     []string{"string"},
			"confidence_score": "float (0-1)",
		},
		"response_suggestion": "string",
	}

	client := openaijsonwrapper.NewOpenAIClientBuilder(apiKey).
		BaseURL(baseURL).
		Build()

	wrapper := openaijsonwrapper.New(
		client,
		modelName,
		targetStructure,
		nil,
		"",
	)

	messages := []openaijsonwrapper.Message{
		{
			Role:    "user",
			Content: openaijsonwrapper.MessageContent{String: strPtr("GitHub Copilot 是一款非常棒的 AI 编程助手，它能极大地提高代码生产力，虽然偶尔会有小瑕疵，但整体瑕不掩瑜。")},
		},
	}

	requirements := []string{
		"情感分析必须细化到分值",
		"key_entities 至少提取两个",
		"response_suggestion 必须是中文",
	}

	background := "你是一个专业级的产品评论分析专家，擅长从用户反馈中提取结构化洞察。"

	fmt.Println("--- 正在发送请求到 LLM ---")

	result, err := wrapper.Chat(messages, openaijsonwrapper.ChatOptions{
		Requirements: joinStrs(requirements, "\n"),
		Background:   background,
	})
	if err != nil {
		log.Fatalf("请求失败: %v", err)
	}

	fmt.Println("\n--- 解析结果 ---")
	if result.Error != nil {
		fmt.Println("解析失败!")
		fmt.Println("错误信息:", *result.Error)
		fmt.Println("原始响应内容:\n", result.RawContent)
	} else {
		fmt.Println("成功解析数据:")
		fmt.Println(string(result.Data))
		fmt.Println("\n思维链/推理过程:")
		fmt.Println(result.Reasoning)
	}
}

func strPtr(s string) *string {
	return &s
}

func joinStrs(strs []string, sep string) string {
	if len(strs) == 0 {
		return ""
	}
	result := strs[0]
	for i := 1; i < len(strs); i++ {
		result += sep + strs[i]
	}
	return result
}
