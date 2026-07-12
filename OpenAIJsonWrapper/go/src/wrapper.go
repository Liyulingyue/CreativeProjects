package openaijsonwrapper

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/sashabaranov/go-openai"
)

const ToolMarkerStart = "```json"
const ToolMarkerEnd = "```"

type ContentPartType string

const (
	ContentPartTypeText      ContentPartType = "text"
	ContentPartTypeImageURL  ContentPartType = "image_url"
	ContentPartTypeImagePath ContentPartType = "image_path"
)

type ContentPart struct {
	Type      ContentPartType `json:"type"`
	Text      string          `json:"text,omitempty"`
	ImageURL  any             `json:"image_url,omitempty"`
	ImagePath string          `json:"image_path,omitempty"`
}

type ImageURLValue struct {
	URL    string `json:"url"`
	Detail string `json:"detail,omitempty"`
}

type MessageContent struct {
	String *string
	Array  []ContentPart
}

func (m *MessageContent) UnmarshalJSON(data []byte) error {
	var str string
	if err := json.Unmarshal(data, &str); err == nil {
		m.String = &str
		return nil
	}

	var arr []ContentPart
	if err := json.Unmarshal(data, &arr); err == nil {
		m.Array = arr
		return nil
	}

	return nil
}

func (m MessageContent) MarshalJSON() ([]byte, error) {
	if m.String != nil {
		return json.Marshal(*m.String)
	}
	if m.Array != nil {
		return json.Marshal(m.Array)
	}
	return []byte("null"), nil
}

type Message struct {
	Role    string          `json:"role"`
	Content MessageContent  `json:"content"`
}

type ChatResult struct {
	Reasoning  string          `json:"reasoning"`
	Data       json.RawMessage `json:"data"`
	Error      *string         `json:"error"`
	RawContent string          `json:"raw_content"`
	ResponseID *string         `json:"response_id,omitempty"`
}

type OpenAIWrapper struct {
	client          *openai.Client
	model           string
	targetStructure any
	requirements    []string
	background     *string
}

type ChatOptions struct {
	TargetStructure   any
	Requirements      string
	ExtraRequirements string
	Background        string
	Model             string
}

func New(
	client *openai.Client,
	model string,
	targetStructure any,
	requirements []string,
	background string,
) *OpenAIWrapper {
	return &OpenAIWrapper{
		client:          client,
		model:           model,
		targetStructure: targetStructure,
		requirements:    requirements,
	}
}

func (w *OpenAIWrapper) buildSystemPrompt(targetStructure any, requirements []string, background *string) string {
	structureStr, _ := json.MarshalIndent(targetStructure, "", "  ")
	prompt := "You are a helpful assistant that MUST output your response in a specific JSON format.\n"

	if background != nil {
		prompt += fmt.Sprintf("\nBackground Information:\n%s\n", *background)
	}

	prompt += fmt.Sprintf("\nThe required JSON structure is:\n%s\n\n", string(structureStr))

	if len(requirements) > 0 {
		reqText := ""
		for _, r := range requirements {
			reqText += fmt.Sprintf("- %s\n", r)
		}
		prompt += fmt.Sprintf("Specific Requirements:\n%s\n", reqText)
	}

	prompt += fmt.Sprintf(
		"Rules:\n"+
			"1. Your final JSON data MUST be wrapped between '%s' and '%s' markdown blocks.\n"+
			"2. Everything before the code block is considered your reasoning or conversational text.\n"+
			"3. Ensure the JSON inside the block is valid and matches the requested structure strictly.\n",
		ToolMarkerStart, ToolMarkerEnd,
	)

	return prompt
}

func parseContent(text string) (string, json.RawMessage, *string) {
	if text == "" {
		return "", nil, strPtr("Empty content")
	}

	thinkEndMarker := "</think>"
	if idx := strings.Index(text, thinkEndMarker); idx != -1 {
		text = strings.TrimSpace(text[idx+len(thinkEndMarker):])
	}

	mdPattern := regexp.MustCompile("```json\\s*([\\s\\S]*?)\\s*```")
	if matches := mdPattern.FindStringSubmatch(text); len(matches) > 1 {
		inner := strings.TrimSpace(matches[1])
		reasoning := text[:strings.Index(text, matches[0])]

		if parsed, err := parseJSON(inner); err == nil {
			return reasoning, parsed, nil
		}

		cleaned := regexp.MustCompile(",(\\s*[\\]}])").ReplaceAllString(inner, "$1")
		if parsed, err := parseJSON(cleaned); err == nil {
			return reasoning, parsed, nil
		}

		errMsg := fmt.Sprintf("JSON parse error in markdown block")
		return reasoning, nil, &errMsg
	}

	for _, pair := range [][]string{{"[", "]"}, {"{", "}"}} {
		opener, closer := pair[0], pair[1]
		if idx := strings.LastIndex(text, opener); idx != -1 {
			cand := strings.TrimSpace(text[idx:])
			if lastCloser := strings.LastIndex(cand, closer); lastCloser != -1 {
				cand = cand[:lastCloser+1]
				if parsed, err := parseJSON(cand); err == nil {
					reasoning := strings.TrimSpace(text[:idx])
					return reasoning, parsed, nil
				}
			}
		}
	}

	return text, nil, strPtr("No JSON structure found")
}

func parseJSON(s string) (json.RawMessage, error) {
	var v any
	if err := json.Unmarshal([]byte(s), &v); err != nil {
		return nil, err
	}
	return json.Marshal(v)
}

func (w *OpenAIWrapper) encodeImage(imageSource string) (string, error) {
	if strings.HasPrefix(imageSource, "http://") || strings.HasPrefix(imageSource, "https://") {
		return imageSource, nil
	}

	path := filepath.Clean(imageSource)
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return "", fmt.Errorf("image not found: %s", path)
	}

	ext := strings.ToLower(filepath.Ext(path))
	mime := map[string]string{
		".jpg":  "image/jpeg",
		".jpeg": "image/jpeg",
		".png":  "image/png",
		".gif":  "image/gif",
		".webp": "image/webp",
		".bmp":  "image/bmp",
	}[ext]
	if mime == "" {
		mime = "image/jpeg"
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("failed to read image: %w", err)
	}

	encoded := encodeBase64(data)
	return fmt.Sprintf("data:%s;base64,%s", mime, encoded), nil
}

func encodeBase64(data []byte) string {
	const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
	result := ""
	for i := 0; i < len(data); i += 3 {
		var n uint32
		remaining := len(data) - i
		n |= uint32(data[i]) << 16
		if remaining > 1 {
			n |= uint32(data[i+1]) << 8
		}
		if remaining > 2 {
			n |= uint32(data[i+2])
		}

		result += string(alphabet[n>>18&63]) + string(alphabet[n>>12&63])
		if remaining > 1 {
			result += string(alphabet[n>>6&63])
		} else {
			result += "="
		}
		if remaining > 2 {
			result += string(alphabet[n&63])
		} else {
			result += "="
		}
	}
	return result
}

func (w *OpenAIWrapper) normalizeContentPart(part ContentPart) (openai.ChatCompletionMessageContentPart, error) {
	switch part.Type {
	case ContentPartTypeText:
		return openai.ChatCompletionMessageContentPart{
			Type: openai.ChatCompletionMessageContentPartTypeText,
			Text: part.Text,
		}, nil

	case ContentPartTypeImagePath:
		url, err := w.encodeImage(part.ImagePath)
		if err != nil {
			return openai.ChatCompletionMessageContentPart{}, err
		}
		return openai.ChatCompletionMessageContentPart{
			Type: openai.ChatCompletionMessageContentPartTypeImageURL,
			ImageURL: &openai.ImageURL{
				URL: url,
			},
		}, nil

	case ContentPartTypeImageURL:
		switch v := part.ImageURL.(type) {
		case string:
			if strings.HasPrefix(v, "http://") || strings.HasPrefix(v, "https://") || strings.HasPrefix(v, "data:") {
				return openai.ChatCompletionMessageContentPart{
					Type: openai.ChatCompletionMessageContentPartTypeImageURL,
					ImageURL: &openai.ImageURL{
						URL: v,
					},
				}, nil
			}
			url, err := w.encodeImage(v)
			if err != nil {
				return openai.ChatCompletionMessageContentPart{}, err
			}
			return openai.ChatCompletionMessageContentPart{
				Type: openai.ChatCompletionMessageContentPartTypeImageURL,
				ImageURL: &openai.ImageURL{
					URL: url,
				},
			}, nil
		case ImageURLValue:
			if strings.HasPrefix(v.URL, "http://") || strings.HasPrefix(v.URL, "https://") || strings.HasPrefix(v.URL, "data:") {
				return openai.ChatCompletionMessageContentPart{
					Type: openai.ChatCompletionMessageContentPartTypeImageURL,
					ImageURL: &openai.ImageURL{
						URL:    v.URL,
						Detail: v.Detail,
					},
				}, nil
			}
			url, err := w.encodeImage(v.URL)
			if err != nil {
				return openai.ChatCompletionMessageContentPart{}, err
			}
			return openai.ChatCompletionMessageContentPart{
				Type: openai.ChatCompletionMessageContentPartTypeImageURL,
				ImageURL: &openai.ImageURL{
					URL:    url,
					Detail: v.Detail,
				},
			}, nil
		}
	}
	return openai.ChatCompletionMessageContentPart{}, fmt.Errorf("unknown content part type")
}

func (w *OpenAIWrapper) normalizeMessage(message Message) (openai.ChatCompletionMessage, error) {
	var content any

	if message.Content.String != nil {
		content = *message.Content.String
	} else if message.Content.Array != nil {
		parts := make([]openai.ChatCompletionMessageContentPart, 0, len(message.Content.Array))
		for _, part := range message.Content.Array {
			normalized, err := w.normalizeContentPart(part)
			if err != nil {
				return openai.ChatCompletionMessage{}, err
			}
			parts = append(parts, normalized)
		}
		content = parts
	}

	return openai.ChatCompletionMessage{
		Role:    message.Role,
		Content: content,
	}, nil
}

func (w *OpenAIWrapper) Chat(messages []Message, options ChatOptions) (*ChatResult, error) {
	target := w.targetStructure
	if options.TargetStructure != nil {
		target = options.TargetStructure
	}
	if target == nil {
		return nil, fmt.Errorf("targetStructure must be provided")
	}

	reqs := w.requirements
	if options.Requirements != "" {
		reqs = append(reqs, options.Requirements)
	}
	if options.ExtraRequirements != "" {
		reqs = append(reqs, options.ExtraRequirements)
	}

	bg := w.background
	if options.Background != "" {
		bg = &options.Background
	}

	model := w.model
	if options.Model != "" {
		model = options.Model
	}

	systemPrompt := w.buildSystemPrompt(target, reqs, bg)

	newMessages := make([]openai.ChatCompletionMessage, 0, len(messages)+1)
	hasSystem := false

	for _, m := range messages {
		if m.Role == "system" {
			var sysContent string
			if m.Content.String != nil {
				sysContent = systemPrompt + "\n\n" + *m.Content.String
			} else {
				arrContent, _ := json.Marshal(m.Content.Array)
				sysContent = systemPrompt + "\n\n" + string(arrContent)
			}
			newMessages = append(newMessages, openai.ChatCompletionMessage{
				Role:    openai.ChatMessageRoleSystem,
				Content: sysContent,
			})
			hasSystem = true
		} else {
			normalized, err := w.normalizeMessage(m)
			if err != nil {
				return nil, err
			}
			newMessages = append(newMessages, normalized)
		}
	}

	if !hasSystem {
		newMessages = append([]openai.ChatCompletionMessage{{
			Role:    openai.ChatMessageRoleSystem,
			Content: systemPrompt,
		}}, newMessages...)
	}

	req := openai.ChatCompletionRequest{
		Model:    model,
		Messages: newMessages,
	}

	response, err := w.client.CreateChatCompletion(nil, req)
	if err != nil {
		errStr := err.Error()
		return &ChatResult{
			Error:      &errStr,
			RawContent: "",
		}, nil
	}

	choice := response.Choices[0]
	content := ""
	if choice.Message.Content != "" {
		content = choice.Message.Content
	}

	reasoning, data, parseErr := parseContent(content)

	return &ChatResult{
		Reasoning:  reasoning,
		Data:       data,
		Error:      parseErr,
		RawContent: content,
		ResponseID: &response.ID,
	}, nil
}

func strPtr(s string) *string {
	return &s
}
