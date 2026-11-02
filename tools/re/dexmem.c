#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>

static unsigned le32(const unsigned char*p){return (unsigned)p[0]|((unsigned)p[1]<<8)|((unsigned)p[2]<<16)|((unsigned)p[3]<<24);}

int main(int argc,char**argv){
    if(argc<3){fprintf(stderr,"usage: %s <pid> <outdir>\n",argv[0]);return 2;}
    int pid=atoi(argv[1]); const char*out=argv[2];
    char mp[96], mmp[96]; snprintf(mp,sizeof mp,"/proc/%d/maps",pid); snprintf(mmp,sizeof mmp,"/proc/%d/mem",pid);
    FILE*f=fopen(mp,"r"); if(!f){perror("maps");return 1;}
    int fd=open(mmp,O_RDONLY); if(fd<0){perror("mem");return 1;}
    const size_t CH=4u<<20; unsigned char*buf=malloc(CH); if(!buf){perror("malloc");return 1;}
    char line[1024]; int idx=0; unsigned long long total=0; int nmatches=0;
    while(fgets(line,sizeof line,f)){
        unsigned long a=0,b=0; char perms[8]={0}; char rest[768]={0};
        if(sscanf(line,"%lx-%lx %7s %*x %*s %*s %767[^\n]",&a,&b,perms,rest)<3) continue;
        if(perms[0]!='r') continue;
        if(b<=a) continue;
        if(b-a > (1UL<<31)) continue;
        unsigned long off=a;
        while(off<b){
            size_t want=(b-off)>CH?CH:(size_t)(b-off);
            ssize_t n=pread(fd,buf,want,(off_t)off);
            if(n<=0) break;
            total+=n;
            for(ssize_t i=0;i+112<=(ssize_t)n;i++){
                const unsigned char*p=buf+i;
                int dex = (memcmp(p,"dex\n035",8)==0||memcmp(p,"dex\n036",8)==0||memcmp(p,"dex\n037",8)==0||memcmp(p,"dex\n038",8)==0||memcmp(p,"dex\n039",8)==0);
                int cdex = (memcmp(p,"cdex001",7)==0);
                if(!dex && !cdex) continue;
                unsigned fsz=le32(p+0x20);
                if(cdex){ if(fsz<0x70||fsz>(64u<<20)) continue; }
                else { if(le32(p+0x28)!=0x12345678u) continue; if(fsz<0x70||fsz>(64u<<20)) continue; }
                char fn[512]; snprintf(fn,sizeof fn,"%s/d%03d_%llx_%u_%s.bin",out,idx++,(unsigned long long)(off+i),fsz,dex?"dex":"cdex");
                unsigned char*blob=malloc(fsz);
                if(!blob) continue;
                ssize_t got=pread(fd,blob,fsz,(off_t)(off+i));
                if(got>0){ FILE*o=fopen(fn,"wb"); if(o){ size_t w=fwrite(blob,1,got,o); (void)w; fclose(o); printf("%s va=%llx size=%u got=%zd\n",fn,(unsigned long long)(off+i),fsz,got); fflush(stdout);} }
                free(blob); nmatches++;
                i+=(ssize_t)fsz-1;
            }
            off+=n;
        }
    }
    fclose(f); close(fd);
    printf("scanned=%llu matches=%d\n",total,nmatches);
    return 0;
}
