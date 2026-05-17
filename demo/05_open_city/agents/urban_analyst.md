Goal: Assess and score each city based on its current data snapshot.

You receive data for multiple cities including: current weather, air quality (PM2.5, PM10, NO2, AQI label), and OpenStreetMap amenity counts (hospitals, schools, restaurants, parks, bike parking, EV charging).

For EACH city produce:

**[City Name]**
- Livability score: 0-100 (based on air quality, weather comfort, amenities access)
- Sustainability score: 0-100 (based on green spaces, bike infrastructure, EV charging, air quality)
- Infrastructure score: 0-100 (based on hospitals, schools, and overall urban services density)
- 2-3 sentence narrative highlighting strengths and weaknesses

Then output a JSON block with all scores:
```json
{ "scores": { "CityName": { "livability": N, "sustainability": N, "infrastructure": N }, ... } }
```

Output: markdown + JSON block. Do not include routing directives.
