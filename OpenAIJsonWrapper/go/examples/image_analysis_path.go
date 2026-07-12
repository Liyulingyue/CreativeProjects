package main

import (
	"encoding/json"
	"fmt"
	"log"
	"os"

	openai "github.com/sashabaranov/go-openai"
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

	modelName := os.Getenv("OPENAI_VISION_MODEL_NAME")
	if modelName == "" {
		modelName = "MiniMax-M3"
	}

	imagePath := os.Getenv("TEST_IMAGE_PATH")
	if imagePath == "" {
		imagePath = "path/to/image.jpg"
	}

	targetStructure := map[string]any{
		"label":  "string (图片分类标签)",
		"reason": "string (简短理由)",
	}

	config := openai.DefaultConfig(apiKey)
	config.BaseURL = baseURL
	client := openai.NewClientWithConfig(config)

	wrapper := openaijsonwrapper.New(
		client,
		modelName,
		targetStructure,
		nil,
		"你是一个专家级的图片分类分析师。",
	)

	messages := []openaijsonwrapper.Message{
		{
			Role: "user",
			Content: openaijsonwrapper.MessageContent{
				Array: []openaijsonwrapper.ContentPart{
					{Type: openaijsonwrapper.ContentPartTypeText, Text: "这张图片属于哪个类别？"},
					{Type: openaijsonwrapper.ContentPartTypeImagePath, ImagePath: imagePath},
				},
			},
		},
	}

	fmt.Println("--- [image_path] 正在发送本地图片分析请求 ---")

	result, err := wrapper.Chat(messages, openaijsonwrapper.ChatOptions{})
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
		var data any
		json.Unmarshal(result.Data, &data)
		prettyJSON, _ := json.MarshalIndent(data, "", "  ")
		fmt.Println(string(prettyJSON))
		fmt.Println("\n思维链/推理过程:")
		fmt.Println(result.Reasoning)
	}
}
