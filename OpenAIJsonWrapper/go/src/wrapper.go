package openaijsonwrapper

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

type OpenAIJsonWrapper struct {
	client           OpenAIClient
	model            string
	targetStructure  any
	requirements      []string
	background       *string
}

type ChatOptions struct {
	TargetStructure   any
	Requirements      string
	ExtraRequirements string
	Background        string
	Model             string
}

func New(
	client OpenAIClient,
	model string,
	targetStructure any,
	requirements []string,
	background string,
) *OpenAIJsonWrapper {
	return &OpenAIJsonWrapper{
		client:          client,
		model:           model,
		targetStructure: targetStructure,
		requirements:    requirements,
	}
}

func toList(s *string) []string {
	if s == nil {
		return nil
	}
	return []string{*s}
}

func (w *OpenAIJsonWrapper) buildSystemPrompt(targetStructure any, requirements []string, background *string) string {
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

func (w *OpenAIJsonWrapper) encodeImage(imageSource string) (string, error) {
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

func (w *OpenAIJsonWrapper) normalizeContentPart(part ContentPart) (ContentPart, error) {
	switch part.Type {
	case ContentPartTypeImagePath:
		url, err := w.encodeImage(part.ImagePath)
		if err != nil {
			return part, err
		}
		return ContentPart{
			Type:     ContentPartTypeImageURL,
			ImageURL: ImageURLValue{URL: url},
		}, nil

	case ContentPartTypeImageURL:
		switch v := part.ImageURL.(type) {
		case string:
			if strings.HasPrefix(v, "http://") || strings.HasPrefix(v, "https://") || strings.HasPrefix(v, "data:") {
				return ContentPart{
					Type:     ContentPartTypeImageURL,
					ImageURL: ImageURLValue{URL: v},
				}, nil
			}
			url, err := w.encodeImage(v)
			if err != nil {
				return part, err
			}
			return ContentPart{
				Type:     ContentPartTypeImageURL,
				ImageURL: ImageURLValue{URL: url},
			}, nil
		case ImageURLValue:
			if strings.HasPrefix(v.URL, "http://") || strings.HasPrefix(v.URL, "https://") || strings.HasPrefix(v.URL, "data:") {
				return part, nil
			}
			url, err := w.encodeImage(v.URL)
			if err != nil {
				return part, err
			}
			return ContentPart{
				Type:     ContentPartTypeImageURL,
				ImageURL: ImageURLValue{URL: url, Detail: v.Detail},
			}, nil
		}
	}
	return part, nil
}

func (w *OpenAIJsonWrapper) normalizeMessage(message Message) (Message, error) {
	if message.Content.Array != nil {
		normalized := make([]ContentPart, 0, len(message.Content.Array))
		for _, part := range message.Content.Array {
			np, err := w.normalizeContentPart(part)
			if err != nil {
				return message, err
			}
			normalized = append(normalized, np)
		}
		message.Content.Array = normalized
	}
	return message, nil
}

func (w *OpenAIJsonWrapper) Chat(messages []Message, options ChatOptions) (*ChatResult, error) {
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

	newMessages := make([]Message, 0, len(messages)+1)
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
			newMessages = append(newMessages, Message{
				Role:    "system",
				Content: MessageContent{String: &sysContent},
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
		newMessages = append([]Message{{
			Role:    "system",
			Content: MessageContent{String: &systemPrompt},
		}}, newMessages...)
	}

	request := ChatCompletionRequest{
		Model:    model,
		Messages: newMessages,
	}

	response, err := w.client.ChatCompletionsCreate(request)
	if err != nil {
		errStr := err.Error()
		return &ChatResult{
			Error:      &errStr,
			RawContent: "",
		}, nil
	}

	choice := response.Choices[0]
	content := ""
	if choice.Message.Content != nil {
		content = *choice.Message.Content
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
