package openaijsonwrapper

import "encoding/json"

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
	Reasoning  string           `json:"reasoning"`
	Data       json.RawMessage  `json:"data"`
	Error      *string          `json:"error"`
	RawContent string           `json:"raw_content"`
	ResponseID *string          `json:"response_id,omitempty"`
}

type OpenAIClient interface {
	ChatCompletionsCreate(request ChatCompletionRequest) (*ChatCompletionResponse, error)
}

type ChatCompletionRequest struct {
	Model    string    `json:"model"`
	Messages []Message `json:"messages"`
}

type ChatCompletionResponse struct {
	ID      string   `json:"id"`
	Choices []Choice `json:"choices"`
}

type Choice struct {
	Message ChoiceMessage `json:"message"`
}

type ChoiceMessage struct {
	Content *string `json:"content,omitempty"`
}
