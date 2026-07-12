package main

import (
	"encoding/json"
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

	modelName := os.Getenv("OPENAI_VISION_MODEL_NAME")
	if modelName == "" {
		modelName = "MiniMax-M3"
	}

	imageURL := os.Getenv("TEST_IMAGE_URL")
	if imageURL == "" {
		imageURL = "https://upload.wikimedia.org/wikipedia/commons/thumb/3/3a/Cat03.jpg/1200px-Cat03.jpg"
	}

	targetStructure := map[string]any{
		"score":             "int, 0-100, 代表照片质量评分",
		"style":             "str, 照片风格描述",
		"caption":           "str, 用中文写一句话，不超过30字",
		"main_objects":      "list[str], 至少2个主要物体",
		"blurry":            "str, 照片是否模糊，'模糊'、'略微模糊'、'清晰'三选一",
		"comments":          "str, 对照片的详细评价，至少50字",
		"recommendations":   "str, 对拍摄者的改进建议，至少30字",
	}

	client := openaijsonwrapper.NewOpenAIClientBuilder(apiKey).
		BaseURL(baseURL).
		Build()

	wrapper := openaijsonwrapper.New(
		client,
		modelName,
		targetStructure,
		[]string{
			"照片的评价评分需要基于照片的清晰度、构图、色彩和主题等因素综合评定。",
			"请确保输出的 JSON 严格符合指定的结构和类型要求。",
		},
		"你是一名专业的旅行照片分析师，擅长从图片中分析出丰富的细节和信息。",
	)

	messages := []openaijsonwrapper.Message{
		{
			Role: "user",
			Content: openaijsonwrapper.MessageContent{
				Array: []openaijsonwrapper.ContentPart{
					{Type: openaijsonwrapper.ContentPartTypeText, Text: "请仔细观察这张图片，按指定 JSON 结构输出。"},
					{Type: openaijsonwrapper.ContentPartTypeImageURL, ImageURL: imageURL},
				},
			},
		},
	}

	fmt.Println("--- [image_url] 正在发送多模态请求 ---")

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
