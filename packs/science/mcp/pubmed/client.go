// Package pubmed implements NCBI E-utilities clients for Lumen Science MCP.
package pubmed

import (
	"encoding/json"
	"encoding/xml"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

const (
	eutilsBase   = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils"
	apiVersion   = "eutils-v1"
	defaultAgent = "lumen-science/1.0 (mailto:support@oasisdata2026.xyz)"
)

// Client queries PubMed via NCBI E-utilities.
type Client struct {
	BaseURL    string
	HTTPClient *http.Client
	UserAgent  string
}

// NewClient returns a rate-friendly PubMed client.
func NewClient() *Client {
	return &Client{
		BaseURL: eutilsBase,
		HTTPClient: &http.Client{
			Timeout: 30 * time.Second,
		},
		UserAgent: defaultAgent,
	}
}

// Article is a compact PubMed record.
type Article struct {
	PMID    string   `json:"pmid"`
	Title   string   `json:"title"`
	Authors []string `json:"authors,omitempty"`
	Journal string   `json:"journal,omitempty"`
	Year    string   `json:"year,omitempty"`
	DOI     string   `json:"doi,omitempty"`
}

// SearchArticles runs esearch + esummary and returns compact article rows.
func (c *Client) SearchArticles(query string, maxResults int) ([]Article, error) {
	if strings.TrimSpace(query) == "" {
		return nil, fmt.Errorf("query is required")
	}
	if maxResults <= 0 {
		maxResults = 20
	}
	if maxResults > 100 {
		maxResults = 100
	}
	ids, err := c.esearch(query, maxResults)
	if err != nil {
		return nil, err
	}
	if len(ids) == 0 {
		return []Article{}, nil
	}
	return c.esummary(ids)
}

// FetchAbstract returns title + abstract text for one PMID.
func (c *Client) FetchAbstract(pmid string) (map[string]any, error) {
	pmid = strings.TrimSpace(pmid)
	if pmid == "" {
		return nil, fmt.Errorf("pmid is required")
	}
	u := fmt.Sprintf("%s/efetch.fcgi?db=pubmed&id=%s&retmode=xml", c.BaseURL, url.QueryEscape(pmid))
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	var doc efetchDoc
	if err := xml.Unmarshal(body, &doc); err != nil {
		return nil, fmt.Errorf("parse efetch: %w", err)
	}
	rec := doc.firstArticle()
	if rec.Title == "" && rec.Abstract == "" {
		return nil, fmt.Errorf("pmid %s not found", pmid)
	}
	return map[string]any{
		"pmid":     pmid,
		"title":    rec.Title,
		"abstract": rec.Abstract,
	}, nil
}

func (c *Client) esearch(query string, retmax int) ([]string, error) {
	u := fmt.Sprintf("%s/esearch.fcgi?db=pubmed&term=%s&retmax=%d&retmode=json",
		c.BaseURL, url.QueryEscape(query), retmax)
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	var out struct {
		ESearchResult struct {
			IDList []string `json:"idlist"`
		} `json:"esearchresult"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, fmt.Errorf("parse esearch: %w", err)
	}
	return out.ESearchResult.IDList, nil
}

func (c *Client) esummary(pmids []string) ([]Article, error) {
	u := fmt.Sprintf("%s/esummary.fcgi?db=pubmed&id=%s&retmode=json",
		c.BaseURL, url.QueryEscape(strings.Join(pmids, ",")))
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	var out struct {
		Result map[string]json.RawMessage `json:"result"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, fmt.Errorf("parse esummary: %w", err)
	}
	articles := make([]Article, 0, len(pmids))
	for _, id := range pmids {
		raw, ok := out.Result[id]
		if !ok || string(raw) == "null" {
			continue
		}
		var row struct {
			Title   string `json:"title"`
			Source  string `json:"source"`
			PubDate string `json:"pubdate"`
			Authors []struct {
				Name string `json:"name"`
			} `json:"authors"`
			ArticleIDs []struct {
				IDType string `json:"idtype"`
				Value  string `json:"value"`
			} `json:"articleids"`
		}
		if err := json.Unmarshal(raw, &row); err != nil {
			continue
		}
		a := Article{PMID: id, Title: row.Title, Journal: row.Source}
		if parts := strings.Fields(row.PubDate); len(parts) > 0 {
			a.Year = parts[0]
		}
		for _, au := range row.Authors {
			if au.Name != "" {
				a.Authors = append(a.Authors, au.Name)
			}
		}
		for _, aid := range row.ArticleIDs {
			if aid.IDType == "doi" {
				a.DOI = aid.Value
				break
			}
		}
		articles = append(articles, a)
	}
	return articles, nil
}

func (c *Client) get(rawURL string) ([]byte, error) {
	req, err := http.NewRequest(http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", c.UserAgent)
	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, err
	}
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("pubmed http %d: %s", resp.StatusCode, strings.TrimSpace(string(body)))
	}
	return body, nil
}

type efetchDoc struct {
	ArticleSet struct {
		Articles []efetchArticle `xml:"PubmedArticle"`
	} `xml:"PubmedArticleSet"`
}

type efetchArticle struct {
	Medline struct {
		Article struct {
			Title    string `xml:"ArticleTitle"`
			Abstract struct {
				Text string `xml:"AbstractText"`
			} `xml:"Abstract"`
		} `xml:"Article"`
	} `xml:"MedlineCitation"`
}

func (d *efetchDoc) firstArticle() struct {
	Title, Abstract string
} {
	var out struct{ Title, Abstract string }
	if len(d.ArticleSet.Articles) == 0 {
		return out
	}
	a := d.ArticleSet.Articles[0]
	out.Title = strings.TrimSpace(a.Medline.Article.Title)
	out.Abstract = strings.TrimSpace(a.Medline.Article.Abstract.Text)
	return out
}
