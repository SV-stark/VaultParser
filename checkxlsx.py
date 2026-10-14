import openpyxl, re
wb=openpyxl.load_workbook("shoghi.xlsx")
ws=wb.active
rows=list(ws.iter_rows(values_only=True))
hdr=5  # header at row 6
data=rows[hdr+1:]
print("sheet", ws.title, "data rows after header:", len(data))
cnt=0; partial=0; blanks=0
for r in data:
    v=r[1] if len(r)>1 else None
    if v is None:
        blanks+=1
        continue
    s=str(v)
    if re.match(r"\d{2}-\d{2}-\d{4}$", s):
        cnt+=1
    elif re.match(r"\d{2}-\d{2}-?$", s):
        partial+=1
print("full-date data rows:", cnt)
print("partial-date data rows:", partial)
print("blank rows:", blanks)
print("non-date/nonblank rows:", len(data)-cnt-partial-blanks)
# show a few non-date rows
examples=[r for r in data if r[1] and not re.match(r"\d{2}-\d{2}-\d{4}$", str(r[1]))][:10]
for r in examples:
    print("NONDATE ROW:", r)