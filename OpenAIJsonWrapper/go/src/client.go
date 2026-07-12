package openaijsonwrapper

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
)

type OpenAIClientBuilder struct {
	apiKey   string
	baseURL  string
}

func NewOpenAIClientBuilder(apiKey string) *OpenAIClientBuilder {
	return &OpenAIClientBuilder{
		apiKey:  apiKey,
		baseURL: "https://api.openai.com",
	}
}

func (b *OpenAIClientBuilder) BaseURL(baseURL string) *OpenAIClientBuilder {
	b.baseURL = baseURL
	return b
}

func (b *OpenAIClientBuilder) Build() *HTTPClient {
	return &HTTPClient{
		apiKey:  b.apiKey,
		baseURL: b.baseURL,
		client:  &http.Client{},
	}
}

type HTTPClient struct {
	apiKey  string
	baseURL string
	client  *http.Client
}

func (c *HTTPClient) ChatCompletionsCreate(request ChatCompletionRequest) (*ChatCompletionResponse, error) {
	url := fmt.Sprintf("%s/v1/chat/completions", c.baseURL)

	bodyBytes, err := json.Marshal(request)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal request: %w", err)
	}

	req, err := http.NewRequest("POST", url, bytes.NewReader(bodyBytes))
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}

	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", c.apiKey))
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("HTTP request failed: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to read response: %w", err)
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, string(respBody))
	}

	var response ChatCompletionResponse
	if err := json.Unmarshal(respBody, &response); err != nil {
		return nil, fmt.Errorf("failed to parse response: %w", err)
	}

	return &response, nil
}
