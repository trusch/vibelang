import React, { useState, useMemo, useCallback } from 'react';
import { stdlibData, stdlibCategories } from '../../generated/StdlibDocs';
import './StdlibExplorer.css';

function StdlibExplorer({ onInsert, onPreview, isPreviewPlaying, currentPreview }) {
  const [searchTerm, setSearchTerm] = useState('');
  const [expandedCategories, setExpandedCategories] = useState(new Set(['Drums', 'Bass']));
  const [expandedSubcategories, setExpandedSubcategories] = useState(new Set());
  const [selectedSynthdef, setSelectedSynthdef] = useState(null);

  // Filter synthdefs based on search
  const filteredData = useMemo(() => {
    if (!searchTerm.trim()) return stdlibData;

    const term = searchTerm.toLowerCase();
    const result = {};

    for (const category of stdlibCategories) {
      const categoryData = stdlibData[category];
      if (!categoryData?.subcategories) continue;

      const filteredSubcats = {};
      for (const [subcat, items] of Object.entries(categoryData.subcategories)) {
        const filteredItems = items.filter(item =>
          item.name.toLowerCase().includes(term) ||
          item.description?.toLowerCase().includes(term) ||
          item.genre?.toLowerCase().includes(term) ||
          item.character?.toLowerCase().includes(term)
        );

        if (filteredItems.length > 0) {
          filteredSubcats[subcat] = filteredItems;
        }
      }

      if (Object.keys(filteredSubcats).length > 0) {
        result[category] = { subcategories: filteredSubcats };
      }
    }

    return result;
  }, [searchTerm]);

  // Toggle category expansion
  const toggleCategory = useCallback((category) => {
    setExpandedCategories(prev => {
      const next = new Set(prev);
      if (next.has(category)) {
        next.delete(category);
      } else {
        next.add(category);
      }
      return next;
    });
  }, []);

  // Toggle subcategory expansion
  const toggleSubcategory = useCallback((key) => {
    setExpandedSubcategories(prev => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  // Generate insert code using inline source (WASM doesn't support file imports)
  const generateInsertCode = useCallback((synthdef) => {
    // Use the inline source code instead of imports
    // WASM/browser environment doesn't have filesystem access
    const sourceCode = synthdef.source || '';

    return `// ${synthdef.name} - ${synthdef.character || synthdef.genre || 'stdlib synthdef'}
${sourceCode}

let ${synthdef.name}_voice = voice("${synthdef.name}")
    .synth("${synthdef.name}")
    .gain(db(-6));
`;
  }, []);

  // Count items in category
  const countItems = useCallback((categoryData) => {
    if (!categoryData?.subcategories) return 0;
    return Object.values(categoryData.subcategories)
      .reduce((sum, items) => sum + items.length, 0);
  }, []);

  const categories = Object.keys(filteredData);

  return (
    <div className="stdlib-explorer">
      {/* Search */}
      <div className="stdlib-search">
        <input
          type="text"
          placeholder="Search sounds..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="stdlib-search-input"
        />
        {searchTerm && (
          <button
            className="stdlib-search-clear"
            onClick={() => setSearchTerm('')}
            title="Clear search"
          >
            x
          </button>
        )}
      </div>

      {/* Tree */}
      <div className="stdlib-tree">
        {categories.length === 0 ? (
          <div className="stdlib-empty">No sounds found</div>
        ) : (
          categories.map(category => {
            const categoryData = filteredData[category];
            const isExpanded = expandedCategories.has(category);
            const itemCount = countItems(categoryData);

            return (
              <div key={category} className="stdlib-category">
                <div
                  className={`stdlib-category-header ${isExpanded ? 'expanded' : ''}`}
                  onClick={() => toggleCategory(category)}
                >
                  <span className="stdlib-chevron">{isExpanded ? '>' : '>'}</span>
                  <span className="stdlib-category-name">{category}</span>
                  <span className="stdlib-category-count">{itemCount}</span>
                </div>

                {isExpanded && categoryData?.subcategories && (
                  <div className="stdlib-subcategories">
                    {Object.entries(categoryData.subcategories).map(([subcat, items]) => {
                      const subcatKey = `${category}/${subcat}`;
                      const isSubExpanded = expandedSubcategories.has(subcatKey) || searchTerm;

                      return (
                        <div key={subcatKey} className="stdlib-subcategory">
                          <div
                            className={`stdlib-subcategory-header ${isSubExpanded ? 'expanded' : ''}`}
                            onClick={() => toggleSubcategory(subcatKey)}
                          >
                            <span className="stdlib-chevron-small">{isSubExpanded ? '-' : '+'}</span>
                            <span className="stdlib-subcategory-name">{subcat}</span>
                            <span className="stdlib-subcategory-count">{items.length}</span>
                          </div>

                          {isSubExpanded && (
                            <div className="stdlib-items">
                              {items.map(item => (
                                <div
                                  key={item.name}
                                  className={`stdlib-item ${selectedSynthdef?.name === item.name ? 'selected' : ''}`}
                                  onClick={() => setSelectedSynthdef(item)}
                                >
                                  <span className="stdlib-item-name">{item.name}</span>
                                </div>
                              ))}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Detail panel */}
      {selectedSynthdef && (
        <div className="stdlib-detail">
          <div className="stdlib-detail-header">
            <h3 className="stdlib-detail-name">{selectedSynthdef.name}</h3>
            <span className="stdlib-detail-type">{selectedSynthdef.type}</span>
          </div>

          {selectedSynthdef.character && (
            <p className="stdlib-detail-desc">{selectedSynthdef.character}</p>
          )}

          {selectedSynthdef.genre && (
            <div className="stdlib-detail-genre">
              <span className="stdlib-label">Genre:</span> {selectedSynthdef.genre}
            </div>
          )}

          {selectedSynthdef.params && selectedSynthdef.params.length > 0 && (
            <div className="stdlib-detail-params">
              <span className="stdlib-label">Parameters:</span>
              <ul>
                {selectedSynthdef.params.slice(0, 6).map(p => (
                  <li key={p.name}>
                    <span className="param-name">{p.name}</span>
                    <span className="param-value">= {p.default}</span>
                  </li>
                ))}
                {selectedSynthdef.params.length > 6 && (
                  <li className="param-more">+{selectedSynthdef.params.length - 6} more</li>
                )}
              </ul>
            </div>
          )}

          <div className="stdlib-detail-actions">
            <button
              className={`stdlib-preview-btn ${isPreviewPlaying && currentPreview === selectedSynthdef.name ? 'playing' : ''}`}
              onClick={() => onPreview?.(selectedSynthdef)}
              title="Preview sound"
            >
              {isPreviewPlaying && currentPreview === selectedSynthdef.name ? 'Stop' : 'Play'}
            </button>
            <button
              className="stdlib-insert-btn"
              onClick={() => onInsert?.(generateInsertCode(selectedSynthdef))}
              title="Insert into editor"
            >
              Insert
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default StdlibExplorer;
